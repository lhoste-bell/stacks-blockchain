/*
 copyright: (c) 2013-2019 by Blockstack PBC, a public benefit corporation.

 This file is part of Blockstack.

 Blockstack is free software. You may redistribute or modify
 it under the terms of the GNU General Public License as published by
 the Free Software Foundation, either version 3 of the License or
 (at your option) any later version.

 Blockstack is distributed in the hope that it will be useful,
 but WITHOUT ANY WARRANTY, including without the implied warranty of
 MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 GNU General Public License for more details.

 You should have received a copy of the GNU General Public License
 along with Blockstack. If not, see <http://www.gnu.org/licenses/>.
*/

use rusqlite::Row;
use rusqlite::types::ToSql;

use std::io;
use std::io::prelude::*;
use std::fmt;
use std::fs;
use std::collections::HashMap;

use chainstate::burn::ConsensusHash;

use chainstate::stacks::Error;
use chainstate::stacks::*;
use chainstate::stacks::db::*;

use std::path::{Path, PathBuf};

use util::db::Error as db_error;
use util::db::{
    FromRow,
    FromColumn,
    DBConn,
    query_row,
    query_rows,
    query_row_columns,
    query_count
};

use core::FIRST_STACKS_BLOCK_HASH;
use core::FIRST_BURNCHAIN_CONSENSUS_HASH;

impl FromRow<StacksBlockHeader> for StacksBlockHeader {
    fn from_row<'a>(row: &'a Row) -> Result<StacksBlockHeader, db_error> {
        let version : u8 = row.get("version");
        let total_burn_str : String = row.get("total_burn");
        let total_work_str : String = row.get("total_work");
        let proof : VRFProof = VRFProof::from_column(row, "proof")?;
        let parent_block = BlockHeaderHash::from_column(row, "parent_block")?;
        let parent_microblock = BlockHeaderHash::from_column(row, "parent_microblock")?;
        let parent_microblock_sequence : u16 = row.get("parent_microblock_sequence");
        let tx_merkle_root = Sha512Trunc256Sum::from_column(row, "tx_merkle_root")?;
        let state_index_root = TrieHash::from_column(row, "state_index_root")?;
        let microblock_pubkey_hash = Hash160::from_column(row, "microblock_pubkey_hash")?;

        let block_hash = BlockHeaderHash::from_column(row, "block_hash")?;

        let total_burn = total_burn_str.parse::<u64>().map_err(|_e| db_error::ParseError)?;
        let total_work = total_work_str.parse::<u64>().map_err(|_e| db_error::ParseError)?;

        let header = StacksBlockHeader {
            version,
            total_work: StacksWorkScore { burn: total_burn, work: total_work },
            proof,
            parent_block,
            parent_microblock,
            parent_microblock_sequence,
            tx_merkle_root,
            state_index_root,
            microblock_pubkey_hash
        };

        if block_hash != FIRST_STACKS_BLOCK_HASH && header.block_hash() != block_hash {
            return Err(db_error::ParseError);
        }

        Ok(header)
    }
}

impl FromRow<StacksMicroblockHeader> for StacksMicroblockHeader {
    fn from_row<'a>(row: &'a Row) -> Result<StacksMicroblockHeader, db_error> {
        let version : u8 = row.get("version");
        let sequence : u16 = row.get("sequence");
        let prev_block = BlockHeaderHash::from_column(row, "prev_block")?;
        let tx_merkle_root = Sha512Trunc256Sum::from_column(row, "tx_merkle_root")?;
        let signature = MessageSignature::from_column(row, "signature")?;

        let microblock_hash = BlockHeaderHash::from_column(row, "microblock_hash")?;

        let microblock_header = StacksMicroblockHeader {
           version,
           sequence,
           prev_block,
           tx_merkle_root,
           signature
        };
        
        if microblock_hash != microblock_header.block_hash() {
            return Err(db_error::ParseError);
        }

        Ok(microblock_header)
    }
}

impl StacksChainState {
    /// Insert a block header that is paired with an already-existing block commit and snapshot
    pub fn insert_stacks_block_header<'a>(tx: &mut StacksDBTx<'a>, tip_info: &StacksHeaderInfo) -> Result<(), Error> {
        assert_eq!(tip_info.block_height, tip_info.anchored_header.total_work.work);
        assert!(tip_info.burn_header_timestamp < i64::max_value() as u64);

        let header = &tip_info.anchored_header;
        let index_root = &tip_info.index_root;
        let consensus_hash = &tip_info.consensus_hash;
        let burn_header_hash = &tip_info.burn_header_hash;
        let block_height = tip_info.block_height;
        let burn_header_height = tip_info.burn_header_height;
        let burn_header_timestamp = tip_info.burn_header_timestamp;

        let total_work_str = format!("{}", header.total_work.work);
        let total_burn_str = format!("{}", header.total_work.burn);
        let total_liquid_stx_str = format!("{}", tip_info.total_liquid_ustx);
        let block_hash = header.block_hash();

        let index_block_hash = StacksBlockHeader::make_index_block_hash(&consensus_hash, &block_hash);

        assert!(block_height < (i64::max_value() as u64));

        let args: &[&dyn ToSql] = &[
            &header.version, &total_burn_str, &total_work_str, &header.proof, &header.parent_block, &header.parent_microblock, &header.parent_microblock_sequence,
            &header.tx_merkle_root, &header.state_index_root, &header.microblock_pubkey_hash,
            &block_hash, &index_block_hash, &consensus_hash, &burn_header_hash, &(burn_header_height as i64),
            &(burn_header_timestamp as i64), &total_liquid_stx_str, &(block_height as i64), &index_root];

        tx.execute("INSERT INTO block_headers \
                    (version, \
                    total_burn, \
                    total_work, \
                    proof, \
                    parent_block, \
                    parent_microblock, \
                    parent_microblock_sequence, \
                    tx_merkle_root, \
                    state_index_root, \
                    microblock_pubkey_hash, \
                    block_hash, \
                    index_block_hash, \
                    consensus_hash, \
                    burn_header_hash, \
                    burn_header_height, \
                    burn_header_timestamp, \
                    total_liquid_ustx, \
                    block_height, \
                    index_root) \
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)", args)
            .map_err(|e| Error::DBError(db_error::SqliteError(e)))?;

        Ok(())
    }
   
    /// Insert a microblock header that is paired with an already-existing block header
    pub fn insert_stacks_microblock_header<'a>(tx: &mut StacksDBTx<'a>, microblock_header: &StacksMicroblockHeader, parent_block_hash: &BlockHeaderHash, parent_consensus_hash: &ConsensusHash, block_height: u64, index_root: &TrieHash) -> Result<(), Error> {
        assert!(block_height < (i64::max_value() as u64));

        let args: &[&dyn ToSql] = &[&microblock_header.version, &microblock_header.sequence, &microblock_header.prev_block,
                                    &microblock_header.tx_merkle_root, &microblock_header.signature, &microblock_header.block_hash(),
                                    &parent_block_hash, &parent_consensus_hash, &(block_height as i64), &index_root];
        tx.execute("INSERT OR REPLACE INTO microblock_headers \
                    (version, sequence, prev_block, tx_merkle_root, signature, microblock_hash, parent_block_hash, parent_consensus_hash, block_height, index_root) \
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)", args)
            .map_err(|e| Error::DBError(db_error::SqliteError(e)))?;

        Ok(())
    }

    /// Get a stacks header info by burn block and block hash (i.e. by primary key).
    /// Does not get back data about the parent microblock stream.
    pub fn get_anchored_block_header_info(conn: &Connection, consensus_hash: &ConsensusHash, block_hash: &BlockHeaderHash) -> Result<Option<StacksHeaderInfo>, Error> {
        let sql = "SELECT * FROM block_headers WHERE consensus_hash = ?1 AND block_hash = ?2".to_string();
        let args: &[&dyn ToSql] = &[&consensus_hash, &block_hash];
        let mut rows = query_rows::<StacksHeaderInfo, _>(conn, &sql, args).map_err(Error::DBError)?;
        if rows.len() > 1 {
            unreachable!("FATAL: multiple rows for the same block hash")  // should be unreachable, since block_hash/consensus_hash is the primary key
        }

        Ok(rows.pop())
    }

    /// Get a stacks header info by index block hash (i.e. by the hash of the burn block header
    /// hash and the block hash -- the hash of the primary key)
    pub fn get_stacks_block_header_info_by_index_block_hash(conn: &Connection, index_block_hash: &StacksBlockId) -> Result<Option<StacksHeaderInfo>, Error> {
        let sql = "SELECT * FROM block_headers WHERE index_block_hash = ?1".to_string();
        let mut rows = query_rows::<StacksHeaderInfo, _>(conn, &sql, &[&index_block_hash]).map_err(Error::DBError)?;
        let cnt = rows.len();
        if cnt > 1 {
            unreachable!("FATAL: multiple rows for the same block hash")  // should be unreachable, since index_block_hash is unique
        }

        Ok(rows.pop())
    }

    /// Get an ancestor block header
    pub fn get_tip_ancestor<'a>(tx: &mut StacksDBTx<'a>, tip: &StacksHeaderInfo, height: u64) -> Result<Option<StacksHeaderInfo>, Error> {
        assert!(tip.block_height >= height);
        match tx.get_ancestor_block_hash(height, &tip.index_block_hash()).map_err(Error::DBError)? {
            Some(bhh) => {
                StacksChainState::get_stacks_block_header_info_by_index_block_hash(tx, &bhh)
            },
            None => {
                Ok(None)
            }
        }
    }

    /// Get an ancestor block header given an index hash
    pub fn get_index_tip_ancestor<'a>(tx: &mut StacksDBTx<'a>, tip_index_hash: &StacksBlockId, height: u64) -> Result<Option<StacksHeaderInfo>, Error> {
        match tx.get_ancestor_block_hash(height, tip_index_hash).map_err(Error::DBError)? {
            Some(bhh) => {
                StacksChainState::get_stacks_block_header_info_by_index_block_hash(tx, &bhh)
            },
            None => {
                Ok(None)
            }
        }
    }

    /// Get the genesis (boot code) block header
    pub fn get_genesis_header_info(conn: &Connection) -> Result<StacksHeaderInfo, Error> {
        // by construction, only one block can have height 0 in this DB
        let sql = "SELECT * FROM block_headers WHERE consensus_hash = ?1 AND block_height = 0";
        let args : &[&dyn ToSql] = &[&FIRST_BURNCHAIN_CONSENSUS_HASH];
        let row_opt = query_row(conn, sql, args)?;
        Ok(row_opt.expect("BUG: no genesis header info"))
    }
}
