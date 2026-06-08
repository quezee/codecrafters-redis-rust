use crate::resp_value::Value;


pub fn handle_request(request: Value) -> Value {
    if let Value::Arr(Some(arr)) = request {
        if arr.is_empty() {
            return Value::Err("request should be non-empty array".into())
        }
        if let Value::BulkStr(Some(cmd)) = &arr[0] {
            match cmd.to_lowercase().as_str() {
                "ping" => {
                    return Value::Str("PONG".into())
                },
                "echo" => {
                    if arr.len() == 1 {
                        return Value::Err("ECHO with no argument received".into())
                    } else {
                        if let Value::BulkStr(_) = &arr[1] {
                            return arr[1].clone()
                        } else {
                            return Value::Err("ECHO's argument should be a bulk string".into())
                        }
                    }
                },
                _ => {
                    return Value::Err("Unknown command: {cmd}".into())
                }
            }
        }
    }
    Value::Err("request should be a non-null RESP array of bulk strings".into())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_req_is_not_array() {
        let request = Value::BulkStr(Some("hey there".into()));
        let response = handle_request(request);
        assert_eq!(response, Value::Err("request should be a non-null RESP array of bulk strings".into()));
    }

    #[test]
    fn test_req_is_null_array() {
        let request = Value::Arr(None);
        let response = handle_request(request);
        assert_eq!(response, Value::Err("request should be a non-null RESP array of bulk strings".into()));
    }

    #[test]
    fn test_req_is_empty_array() {
        let request = Value::Arr(Some(vec![]));
        let response = handle_request(request);
        assert_eq!(response, Value::Err("request should be non-empty array".into()));
    }

    #[test]
    fn test_ping() {
        let request = Value::Arr(Some(vec![
            Value::BulkStr(Some("PiNg".into()))
        ]));
        let response = handle_request(request);
        assert_eq!(response, Value::Str("PONG".into()));
    }

    #[test]
    fn test_echo() {
        {   // echo with no argument
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("eChO".into()))
            ]));
            let response = handle_request(request);
            assert_eq!(response, Value::Err("ECHO with no argument received".into()));
        }
        {   // echo with wrong argument
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("eChO".into())),
                Value::Str("hello".into())
            ]));
            let response = handle_request(request);
            assert_eq!(response, Value::Err("ECHO's argument should be a bulk string".into()));
        }
        {   // echo with correct argument
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("eChO".into())),
                Value::BulkStr(Some("hello".into()))
            ]));
            let response = handle_request(request);
            assert_eq!(response, Value::BulkStr(Some("hello".into())));
        }
    }
}
