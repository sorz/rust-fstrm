#[test]
fn test_unidirectional_reader() {
    use fstrm::control::ContentType;
    use fstrm::reader::uni_directional::Build;
    use fstrm::reader::*;
    use fstrm::*;

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

    let content_type: ContentType = "test-content-type".into();
    let content_types = vec![content_type.clone()];

    let mut builder = Builder::new(&bytes[..], &content_types);
    let mut reader = builder.build().unwrap();

    assert_eq!(reader.content_type_ref(), Some(&content_type));

    assert_eq!(
        reader.next().unwrap().unwrap(),
        <&str as Into<Payload>>::into("test-content")
    );

    assert!(reader.next().is_none());
}
