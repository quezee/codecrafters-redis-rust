use crate::resp_value::Value;
use crate::resp_parser::RespError;


pub fn handle_request(request: Value) -> Result<Value, RespError> {
    if let Value::Arr(Some(arr)) = request {
        if arr.is_empty() {
            return Ok(Value::Err("request should be non-empty array".into()))
        }
        if let Value::BulkStr(Some(cmd)) = &arr[0] {
            let cmd = String::from_utf8(cmd.clone())?;
            match cmd.to_lowercase().as_str() {
                "ping" => {
                    return Ok(Value::Str("PONG".into()))
                },
                "echo" => {
                    if arr.len() == 1 {
                        return Ok(Value::Err("ECHO with no argument received".into()))
                    } else {
                        if let Value::BulkStr(_) = arr[1] {
                            return Ok(arr[1].clone())
                        } else {
                            return Ok(Value::Err("ECHO's argument should be a bulk string".into()))
                        }
                    }
                },
                _ => {
                    return Ok(Value::Err(format!("Unknown command: {}", cmd)))
                }
            }
        }
    }
    Ok(Value::Err("request should be a non-null RESP array of bulk strings".into()))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_req_is_not_array() {
        let request = Value::BulkStr(Some("hey there".into()));
        let response = handle_request(request);
        assert_eq!(response, Ok(Value::Err("request should be a non-null RESP array of bulk strings".into())));
    }

    #[test]
    fn test_req_is_null_array() {
        let request = Value::Arr(None);
        let response = handle_request(request);
        assert_eq!(response, Ok(Value::Err("request should be a non-null RESP array of bulk strings".into())));
    }

    #[test]
    fn test_req_is_empty_array() {
        let request = Value::Arr(Some(vec![]));
        let response = handle_request(request);
        assert_eq!(response, Ok(Value::Err("request should be non-empty array".into())));
    }

    #[test]
    fn test_cmd_non_utf8() {
        let invalid_cmd = b"hello\xffworld".to_vec();
        let expected_err = String::from_utf8(invalid_cmd.clone()).unwrap_err();

        let request = Value::Arr(Some(vec![
            Value::BulkStr(Some(invalid_cmd))
        ]));
        let response = handle_request(request);
        assert_eq!(response, Err(RespError::FromUtf8(expected_err)));
    }

    #[test]
    fn test_ping() {
        let request = Value::Arr(Some(vec![
            Value::BulkStr(Some("PiNg".into()))
        ]));
        let response = handle_request(request);
        assert_eq!(response, Ok(Value::Str("PONG".into())));
    }

    #[test]
    fn test_echo() {
        {   // echo with no argument
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("eChO".into()))
            ]));
            let response = handle_request(request);
            assert_eq!(response, Ok(Value::Err("ECHO with no argument received".into())));
        }
        {   // echo with wrong argument
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("eChO".into())),
                Value::Str("hello".into())
            ]));
            let response = handle_request(request);
            assert_eq!(response, Ok(Value::Err("ECHO's argument should be a bulk string".into())));
        }
        {   // echo with correct argument
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("eChO".into())),
                Value::BulkStr(Some("hello".into()))
            ]));
            let response = handle_request(request);
            assert_eq!(response, Ok(Value::BulkStr(Some("hello".into()))));
        }
    }
}
