use jackbot_data::error::DataError;
use jackbot_integration::error::SocketError;

#[test]
fn test_data_error_is_terminal() {
    struct TestCase {
        input: DataError,
        expected: bool,
    }

    let tests = vec![
        TestCase {
            // TC0: is terminal w/ DataError::InvalidSequence
            input: DataError::InvalidSequence {
                prev_last_update_id: 0,
                first_update_id: 0,
            },
            expected: true,
        },
        TestCase {
            // TC1: is not terminal w/ DataError::Socket
            input: DataError::from(SocketError::Sink),
            expected: false,
        },
    ];

    for (index, test) in tests.into_iter().enumerate() {
        let actual = test.input.is_terminal();
        assert_eq!(actual, test.expected, "TC{} failed", index);
    }
}
