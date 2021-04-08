pub mod db;
pub mod download;

pub use self::db::AtlasDB;
pub use self::download::AttachmentsDownloader;

use chainstate::stacks::boot::boot_code_id;
use chainstate::stacks::{StacksBlockHeader, StacksBlockId};

use burnchains::Txid;
use chainstate::burn::db::sortdb::SortitionDB;
use chainstate::burn::{BlockHeaderHash, ConsensusHash};
use crate::codec::StacksMessageCodec;
use util::hash::{to_hex, Hash160, MerkleHashFunc};
use vm::types::{QualifiedContractIdentifier, SequenceData, TupleData, Value};

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::hash::{Hash, Hasher};

pub const MAX_ATTACHMENT_INV_PAGES_PER_REQUEST: usize = 8;

lazy_static! {
    pub static ref BNS_CHARS_REGEX: Regex = Regex::new("^([a-z0-9]|[-_])*$").unwrap();
}

#[derive(Debug, Clone)]
pub struct AtlasConfig {
    pub contracts: HashSet<QualifiedContractIdentifier>,
    pub attachments_max_size: u32,
    pub max_uninstantiated_attachments: u32,
    pub uninstantiated_attachments_expire_after: u32,
    pub unresolved_attachment_instances_expire_after: u32,
    pub genesis_attachments: Option<Vec<Attachment>>,
}

impl AtlasConfig {
    pub fn default(mainnet: bool) -> AtlasConfig {
        let mut contracts = HashSet::new();
        contracts.insert(boot_code_id("bns", mainnet));
        AtlasConfig {
            contracts,
            attachments_max_size: 1_048_576,
            max_uninstantiated_attachments: 10_000,
            uninstantiated_attachments_expire_after: 3_600,
            unresolved_attachment_instances_expire_after: 172_800,
            genesis_attachments: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct Attachment {
    pub content: Vec<u8>,
}

impl Attachment {
    pub fn new(content: Vec<u8>) -> Attachment {
        Attachment { content }
    }

    pub fn hash(&self) -> Hash160 {
        Hash160::from_data(&self.content)
    }

    pub fn empty() -> Attachment {
        Attachment { content: vec![] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct AttachmentInstance {
    pub content_hash: Hash160,
    pub attachment_index: u32,
    pub block_height: u64,
    pub index_block_hash: StacksBlockId,
    pub metadata: String,
    pub contract_id: QualifiedContractIdentifier,
    pub tx_id: Txid,
}

impl AttachmentInstance {
    const ATTACHMENTS_INV_PAGE_SIZE: u32 = 64;

    pub fn try_new_from_value(
        value: &Value,
        contract_id: &QualifiedContractIdentifier,
        index_block_hash: StacksBlockId,
        block_height: u64,
        tx_id: Txid,
    ) -> Option<AttachmentInstance> {
        if let Value::Tuple(ref attachment) = value {
            if let Ok(Value::Tuple(ref attachment_data)) = attachment.get("attachment") {
                match (
                    attachment_data.get("hash"),
                    attachment_data.get("attachment-index"),
                ) {
                    (
                        Ok(Value::Sequence(SequenceData::Buffer(content_hash))),
                        Ok(Value::UInt(attachment_index)),
                    ) => {
                        let content_hash = if content_hash.data.is_empty() {
                            Hash160::empty()
                        } else {
                            match Hash160::from_bytes(&content_hash.data[..]) {
                                Some(content_hash) => content_hash,
                                _ => return None,
                            }
                        };
                        let metadata = match attachment_data.get("metadata") {
                            Ok(metadata) => {
                                let mut serialized = vec![];
                                metadata
                                    .consensus_serialize(&mut serialized)
                                    .expect("FATAL: invalid metadata");
                                to_hex(&serialized[..])
                            }
                            _ => String::new(),
                        };
                        let instance = AttachmentInstance {
                            index_block_hash,
                            content_hash,
                            attachment_index: *attachment_index as u32,
                            block_height,
                            metadata,
                            contract_id: contract_id.clone(),
                            tx_id,
                        };
                        return Some(instance);
                    }
                    _ => {}
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
