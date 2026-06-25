use crate::{Storage, Entry};
use crate::resp_value::Value;
use crate::resp_parser::RespError;

use core::time::Duration;
use std::time::Instant;


pub fn handle_request(request: Value, storage: &Storage) -> Result<Value, RespError> {
    if let Value::Arr(Some(mut arr)) = request {
        if arr.is_empty() {
            return Ok(Value::Err("request should be non-empty array".into()))
        }
        let cmd = std::mem::take(&mut arr[0]);
        if let Value::BulkStr(Some(cmd)) = cmd {
            let cmd = String::from_utf8(cmd)?;
            match cmd.to_lowercase().as_str() {
                "ping" => {
                    return Ok(Value::Str("PONG".into()))
                },
                "echo" => {
                    if arr.len() == 1 {
                        return Ok(Value::Err("ECHO with no argument received".into()))
                    } else if arr.len() == 2 {
                        let msg = arr.swap_remove(1);
                        if let Value::BulkStr(_) = msg {
                            return Ok(msg)
                        } else {
                            return Ok(Value::Err("ECHO's argument should be a bulk string".into()))
                        }
                    } else {
                        return Ok(Value::Err(format!("ECHO requires 0 or 1 arguments, provided: {}", arr.len() - 1)))
                    }
                },
                "set" => {
                    if arr.len() != 3 && arr.len() != 5 {
                        return Ok(Value::Err(format!("SET expects 2 or 4 arguments, provided: {}", arr.len() - 1)))
                    } else {
                        // Move the value and key out of `arr` (O(1), no cloning).
                        // - case w 2 arguments: indices 2 then 1 are the last elements at each step, so `swap_remove` just pops them
                        // - case w 4 arguments: the 2 optional arguments are essentially moved to the beginning (preserving order)
                        let value = arr.swap_remove(2);
                        let key = arr.swap_remove(1);
                        if let Value::BulkStr(Some(key)) = key {
                            let mut guard = match storage.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    return Ok(Value::Err(e.to_string()))
                                }
                            };
                            let mut expires_at = None;
                            if arr.len() == 3 {
                                // Process expiration command
                                let exp_type = &arr[1];
                                let exp_time = &arr[2];
                                if let Value::BulkStr(Some(exp_type)) = exp_type {
                                    if let Value::BulkStr(Some(exp_time)) = exp_time {
                                        let exp_type = exp_type.to_ascii_lowercase();
                                        let exp_time: u64 = match std::str::from_utf8(exp_time) {
                                            Ok(str) => match str.parse::<u64>() {
                                                Ok(num) => num,
                                                Err(e) => return Ok(Value::Err(format!("Expiration time parsing error: {}", e)))
                                            },
                                            Err(e) => return Ok(Value::Err(format!("Expiration time parsing error: {}", e)))
                                        };
                                        let dur = match exp_type.as_slice() {
                                            b"px" | b"PX" | b"Px" | b"pX" => Duration::from_millis(exp_time),
                                            b"ex" | b"EX" | b"Ex" | b"eX" => Duration::from_secs(exp_time),
                                            _ => return Ok(Value::Err(format!("Unknown expiration type: {}", String::from_utf8_lossy(&exp_type))))
                                        };
                                        expires_at = Some(Instant::now() + dur);
                                    } else {
                                        return Ok(Value::Err("Unknown expiry time for SET provided".into()))
                                    }
                                } else {
                                    return Ok(Value::Err("Unknown expiry type for SET provided".into()))
                                }
                            }
                            guard.insert(key, Entry::new(value, expires_at));
                            return Ok(Value::Str("OK".into()))
                        } else {
                            return Ok(Value::Err("SET key is expected to be non-null bulk string".into()))
                        }
                    }
                },
                "get" => {
                    if arr.len() != 2 {
                        return Ok(Value::Err(format!("GET expects 1 argument, provided: {}", arr.len() - 1)))
                    } else {
                        let key = arr.swap_remove(1);
                        if let Value::BulkStr(Some(key)) = key {
                            let mut guard = match storage.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    return Ok(Value::Err(e.to_string()))
                                }
                            };
                            match guard.get(&key) {
                                Some(Entry{value, expires_at}) => {
                                    if let Some(expires_at) = expires_at && *expires_at <= Instant::now() {
                                        guard.remove(&key);
                                        return Ok(Value::BulkStr(None))
                                    }
                                    return Ok(value.clone())
                                },
                                None => return Ok(Value::BulkStr(None)),
                            }
                        } else {
                            return Ok(Value::Err("GET key is expected to be non-null bulk string".into()))
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
    use std::collections::HashMap;

    #[test]
    fn test_req_is_not_array() {
        let request = Value::BulkStr(Some("hey there".into()));
        let response = handle_request(request, &Storage::default());
        assert_eq!(response, Ok(Value::Err("request should be a non-null RESP array of bulk strings".into())));
    }

    #[test]
    fn test_req_is_null_array() {
        let request = Value::Arr(None);
        let response = handle_request(request, &Storage::default());
        assert_eq!(response, Ok(Value::Err("request should be a non-null RESP array of bulk strings".into())));
    }

    #[test]
    fn test_req_is_empty_array() {
        let request = Value::Arr(Some(vec![]));
        let response = handle_request(request, &Storage::default());
        assert_eq!(response, Ok(Value::Err("request should be non-empty array".into())));
    }

    #[test]
    fn test_cmd_non_utf8() {
        let invalid_cmd = b"hello\xffworld".to_vec();
        let expected_err = String::from_utf8(invalid_cmd.clone()).unwrap_err();

        let request = Value::Arr(Some(vec![
            Value::BulkStr(Some(invalid_cmd))
        ]));
        let response = handle_request(request, &Storage::default());
        assert_eq!(response, Err(RespError::FromUtf8(expected_err)));
    }

    #[test]
    fn test_ping() {
        let request = Value::Arr(Some(vec![
            Value::BulkStr(Some("PiNg".into()))
        ]));
        let response = handle_request(request, &Storage::default());
        assert_eq!(response, Ok(Value::Str("PONG".into())));
    }

    #[test]
    fn test_echo() {
        {   // echo with no argument
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("eChO".into()))
            ]));
            let response = handle_request(request, &Storage::default());
            assert_eq!(response, Ok(Value::Err("ECHO with no argument received".into())));
        }
        {   // echo with wrong argument
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("eChO".into())),
                Value::Str("hello".into())
            ]));
            let response = handle_request(request, &Storage::default());
            assert_eq!(response, Ok(Value::Err("ECHO's argument should be a bulk string".into())));
        }
        {   // echo with correct argument
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("eChO".into())),
                Value::BulkStr(Some("hello".into()))
            ]));
            let response = handle_request(request, &Storage::default());
            assert_eq!(response, Ok(Value::BulkStr(Some("hello".into()))));
        }
    }

    #[test]
    fn test_set() {
        let storage = Storage::default();
        {   // 0 arguments provided
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("Set".into())),
            ]));
            let response = handle_request(request, &storage);
            assert_eq!(response, Ok(Value::Err("SET expects 2 or 4 arguments, provided: 0".into())));
        }
        {   // wrong type of key provided
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("Set".into())),
                Value::BulkStr(None),
                Value::Str("val".into()),
            ]));
            let response = handle_request(request, &storage);
            assert_eq!(response, Ok(Value::Err("SET key is expected to be non-null bulk string".into())));
        }
        {   // correct case
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("Set".into())),
                Value::BulkStr(Some("key1".into())),
                Value::Str("val1".into()),
            ]));
            let response = handle_request(request, &storage);
            assert_eq!(response, Ok(Value::Str("OK".into())));
            assert_eq!(
                *storage.lock().unwrap(),
                HashMap::from([
                    (b"key1".to_vec(), Value::Str("val1".into()).into())
                ])
            );
        }
        {   // with exp
            let set_request = Value::Arr(Some(vec![
                Value::BulkStr(Some("set".into())),
                Value::BulkStr(Some("foo".into())),
                Value::Str("bar".into()),
                Value::BulkStr(Some("px".into())),
                Value::BulkStr(Some("100".into())),
            ]));
            let response = handle_request(set_request, &storage);
            assert_eq!(response, Ok(Value::Str("OK".into())));

            std::thread::sleep(Duration::from_millis(50));

            let get_request = Value::Arr(Some(vec![
                Value::BulkStr(Some("get".into())),
                Value::BulkStr(Some("foo".into())),
            ]));            
            let response = handle_request(get_request.clone(), &storage);
            assert_eq!(response, Ok(Value::Str("bar".into())));

            std::thread::sleep(Duration::from_millis(51));

            let response = handle_request(get_request, &storage);
            assert_eq!(response, Ok(Value::BulkStr(None)));
        }
    }

    #[test]
    fn test_get() {
        let storage = Storage::default();
        storage.lock().unwrap().insert(
          b"key1".into(),
          Value::Int(100).into()
        );
        {   // 0 arguments provided
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("get".into())),
            ]));
            let response = handle_request(request, &storage);
            assert_eq!(response, Ok(Value::Err("GET expects 1 argument, provided: 0".into())));
        }
        {   // wrong type of key provided
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("gEt".into())),
                Value::Int(1),
            ]));
            let response = handle_request(request, &storage);
            assert_eq!(response, Ok(Value::Err("GET key is expected to be non-null bulk string".into())));
        }
        {   // non-existent key requested
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("gEt".into())),
                Value::BulkStr(Some("key2".into())),
            ]));
            let response = handle_request(request, &storage);
            assert_eq!(response, Ok(Value::BulkStr(None)));
        }
        {   // correct case
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("gEt".into())),
                Value::BulkStr(Some("key1".into())),
            ]));
            let response = handle_request(request, &storage);
            assert_eq!(response, Ok(Value::Int(100)));
            let guard = storage.lock().unwrap();
            assert_eq!(
                *guard,
                HashMap::from([
                    (b"key1".to_vec(), Value::Int(100).into())
                ])
            );
        }
    }
}
