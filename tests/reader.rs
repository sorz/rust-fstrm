use fstrm::reader;
use std::io::Read;

#[test]
fn test_unidirectional_reader() {
    let bytes: [u8; 65] = [
        0, 0, 0, 0, 0, 0, 0, 29, // control frame, length 29
        0, 0, 0, 2, // control type: START
        0, 0, 0, 1, // field type: content type
        0, 0, 0, 17, // field length 17
        116, 101, 115, 116, 45, 99, 111, 110, 116, 101, 110, 116, 45, 116, 121, 112,
        101, // "test-content-type"
        0, 0, 0, 12, // data frame, length 12
        116, 101, 115, 116, 45, 99, 111, 110, 116, 101, 110, 116, // "test-content"
        0, 0, 0, 0, 0, 0, 0, 4, // control frame, length 4
        0, 0, 0, 3, // control type: STOP
    ];

    let reader = reader::reader(&bytes[..]);
    let mut reader = reader.start().unwrap();
    let types = reader.content_types();
    assert_eq!(types.len(), 1);
    assert!(types.contains("test-content-type"));

    let mut frame = reader.read_frame().unwrap().unwrap();
    let mut buf = String::new();
    frame.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "test-content");

    assert!(reader.read_frame().unwrap().is_none());
}
