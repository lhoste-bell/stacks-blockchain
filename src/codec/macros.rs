macro_rules! impl_byte_array_message_codec {
    ($thing:ident, $len:expr) => {
        impl ::codec::StacksMessageCodec for $thing {
            fn consensus_serialize<W: std::io::Write>(
                &self,
                fd: &mut W,
            ) -> Result<(), ::codec::Error> {
                fd.write_all(self.as_bytes())
                    .map_err(::codec::Error::WriteError)
            }
            fn consensus_deserialize<R: std::io::Read>(fd: &mut R) -> Result<$thing, ::codec::Error> {
                let mut buf = [0u8; ($len as usize)];
                fd.read_exact(&mut buf).map_err(::codec::Error::ReadError)?;
                let ret = $thing::from_bytes(&buf).expect("BUG: buffer is not the right size");
                Ok(ret)
            }
        }
    };
}
