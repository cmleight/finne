// use bytes::{Buf, Bytes, BytesMut};
// use tracing;
//
// enum IndexType {
//     Reverse,
//     Range,
//     Geospatial,
// }
//
// struct Index {
//     index_name: String,
//     index_type: IndexType,
//     data_buf: BytesMut,
//     rec_buf: BytesMut
// }
//
// impl Index {
//     fn from_bytes(name: String, mut bytes: Bytes, docbuf: DocBuf) -> Option<Index> {
//         let index_type: IndexType = match bytes.get_u8() {
//             0x1 => IndexType::Reverse,
//             0x2 => IndexType::Range,
//             0x3 => IndexType::Geospatial,
//             x => {
//                 tracing::warn!("Invalid index type: {:?}", x);
//                 return None;
//             },
//         };
//         bytes.advance(1);
//         let rec_length: f64 = bytes.get_f64();
//         if rec_length < 0.0 {
//             tracing::warn!("Invalid index rec length! {:?}", rec_length);
//             return None;
//         }
//         bytes.advance(4);
//         let data_length: f64 = bytes.get_f64();
//         if data_length < 0.0 {
//             tracing::warn!("Invalid index data length! {:?}", data_length);
//             return None;
//         }
//         bytes.advance(4);
//         let rec_buf = BytesMut::from(&bytes[..rec_length as usize]);
//         bytes.advance(rec_length as usize);
//         let data: BytesMut = BytesMut::from(&bytes[..data_length as usize]);
//         bytes.advance(data_length as usize);
//
//         for i in 0..data_length as usize {
//         }
//
//
//     }
// }
//
// fn build_reverse(data_buf: &mut BytesMut, rec_buf: &mut BytesMut) {
//
//     Some(Index{
//         index_name: name.clone(),
//         index_type,
//         data_buf,
//         rec_buf,
//     })
// }
