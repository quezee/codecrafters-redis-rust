use crate::{Storage, StorageEntry};
use crate::resp_value::Value;
use crate::resp_parser::RespError;

use core::time::Duration;
use std::time::Instant;
use std::collections::hash_map::Entry;


pub fn handle_request(request: Value, storage: &Storage) -> Result<Value, RespError> {
    if let Value::Arr(Some(arr)) = request {
        if arr.is_empty() {
            return Ok(Value::Err("request should be non-empty array".into()))
        }
        let mut args = arr.into_iter();
        let cmd = args.next();
        if let Some(Value::BulkStr(Some(cmd))) = cmd {
            let cmd = String::from_utf8(cmd)?;
            match cmd.to_lowercase().as_str() {
                "ping" => {
                    return Ok(Value::Str("PONG".into()))
                },
                "echo" => {
                    match args.next() {
                        Some(echo_arg) => {
                            match echo_arg {
                               Value::BulkStr(_) => return Ok(echo_arg),
                               _ => return Ok(Value::Err("ECHO's argument should be a bulk string".into())),
                            }
                        },
                        None => return Ok(Value::Err("ECHO with no argument received".into())),
                    }
                },
                "get" => {
                    return handle_get_request(&mut args, storage);
                },
                "set" => {
                    return handle_set_request(&mut args, storage);
                },
                "rpush" => {
                    return handle_rpush_request(&mut args, storage);
                },
                "lrange" => {
                    return handle_lrange_request(&mut args, storage);
                },
                _ => {
                    return Ok(Value::Err(format!("Unknown command: {}", cmd)))
                }
            }
        }
    }
    Ok(Value::Err("request should be a non-null RESP array of bulk strings".into()))
}


fn handle_get_request(args: &mut impl Iterator<Item=Value>, storage: &Storage) -> Result<Value, RespError> {
    match args.next() {
        Some(Value::BulkStr(Some(key))) => {
            match storage.lock() {
                Ok(mut g) => {
                    match g.entry(key) {
                        Entry::Occupied(entry) => {
                            if let Some(expires_at) = entry.get().expires_at && expires_at <= Instant::now() {
                                entry.remove();
                                return Ok(Value::BulkStr(None));
                            }
                            return Ok(entry.get().value.clone());
                        },
                        Entry::Vacant(_) => return Ok(Value::BulkStr(None))
                    }
                },
                Err(msg) => {
                    return Ok(Value::Err(format!("Lock poisoned: {}", msg.to_string())))
                }
            }
        },
        None => return Ok(Value::Err("GET expects 1 argument, provided: 0".into())),
        _ => return Ok(Value::Err("GET key is expected to be non-null bulk string".into()))
    }
}

struct SetOptArgs {
    expires_at: Option<Instant>,
}

fn handle_set_request(args: &mut impl Iterator<Item=Value>, storage: &Storage) -> Result<Value, RespError> {
    if let (Some(key), Some(value)) = (args.next(), args.next()) {
        if let Value::BulkStr(Some(key)) = key {
            let opt_args = match extract_set_opt_args(args) {
                Ok(opt_args) => opt_args,
                Err(msg) => return Ok(Value::Err(msg))
            };
            match storage.lock() {
                Ok(mut g) => {
                    g.insert(key, StorageEntry::new(value, opt_args.expires_at));
                    return Ok(Value::Str("OK".into()))
                },
                Err(msg) => {
                    return Ok(Value::Err(format!("Lock poisoned: {}", msg.to_string())))
                }
            }
        } else {
            return Ok(Value::Err("SET key is expected to be non-null bulk string".into()))
        }
    } else {
        return Ok(Value::Err("SET expects at least 2 arguments".into()))
    }

}

fn extract_set_opt_args(args: &mut impl Iterator<Item=Value>) -> Result<SetOptArgs, String> {
    let mut expires_at = None;
    if let Some(Value::BulkStr(Some(arg))) = args.next() {
        match arg.as_slice() {
            b"px" | b"PX" | b"Px" | b"pX" => {
                let exp_time = args.next().ok_or("expiration time not provided")?;
                let exp_time: u64 = convert_bulk_str(exp_time)?;
                expires_at = Some(Instant::now() + Duration::from_millis(exp_time));
            },
            b"ex" | b"EX" | b"Ex" | b"eX" => {
                let exp_time = args.next().ok_or("expiration time not provided")?;
                let exp_time: u64 = convert_bulk_str(exp_time)?;
                expires_at = Some(Instant::now() + Duration::from_secs(exp_time));
            },
            other => return Err(format!("Unknown optional command {}", String::from_utf8_lossy(other)))
        }
    }
    Ok(SetOptArgs{expires_at})
}

fn convert_bulk_str<T>(val: Value) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display
{
    if let Value::BulkStr(Some(val)) = val {
        match std::str::from_utf8(&val) {
            Ok(str) => match str.parse::<T>() {
                Ok(num) => Ok(num),
                Err(e) => Err(format!("BulkStr parsing error: {}", e))
            },
            Err(e) => Err(format!("BulkStr parsing error: {}", e))
        }
    } else {
        return Err("Non-null BulkStr expected".into())
    }
}

fn handle_rpush_request(args: &mut impl Iterator<Item=Value>, storage: &Storage) -> Result<Value, RespError> {
    let key = args.next();
    let mut vals: Vec<Value> = args.collect();

    if let Some(key) = key && vals.len() > 0 {
        if let Value::BulkStr(Some(key)) = key {
            match storage.lock() {
                Ok(mut g) => {
                    let arr = g.entry(key)
                        .or_insert_with(|| Value::Arr(Some(vec![])).into());
                    if let Value::Arr(Some(arr)) = &mut arr.value {
                        arr.append(&mut vals);
                        return Ok(Value::Int(arr.len() as i64));
                    } else {
                        return Ok(Value::Err(format!("WRONGTYPE for list: {:?}", arr.value)))
                    }
                },
                Err(msg) => {
                    return Ok(Value::Err(format!("Lock poisoned: {}", msg.to_string())))
                }
            }
        } else {
            return Ok(Value::Err("RPUSH key is expected to be non-null bulk string".into()))
        }
    } else {
        return Ok(Value::Err("USAGE: RPUSH <key> <vals...>".into()))
    }
}

fn handle_lrange_request(args: &mut impl Iterator<Item=Value>, storage: &Storage) -> Result<Value, RespError> {
    let key = args.next();
    let start = args.next();
    let stop = args.next();

    if let (
        Some(Value::BulkStr(Some(key))),
        Some(start),
        Some(stop)
    ) = (key, start, stop) {
        let start: i64 = match convert_bulk_str(start) {
            Ok(idx) => idx,
            Err(msg) => return Ok(Value::Err(msg)),
        };
        let mut stop: i64 = match convert_bulk_str(stop) {
            Ok(idx) => idx,
            Err(msg) => return Ok(Value::Err(msg)),
        };
        match storage.lock() {
            Ok(g) => {
                let lst = g.get(&key);
                if let Some(StorageEntry{value: Value::Arr(Some(arr)), expires_at: _}) = lst {
                    if start >= arr.len() as i64 || start > stop {
                        return Ok(Value::Arr(Some(vec![])))
                    }
                    if stop >= arr.len() as i64 {
                        stop = arr.len() as i64 - 1;
                    }
                    return Ok(Value::Arr(Some(arr[start as usize..=stop as usize].into())))
                } else {
                    return Ok(Value::Arr(Some(vec![])))
                }
            },
            Err(msg) => {
                return Ok(Value::Err(format!("Lock poisoned: {}", msg.to_string())))
            }
        }
    } else {
        return Ok(Value::Err("USAGE: LRANGE <key> <start> <stop>".into()))
    }
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
            assert_eq!(response, Ok(Value::Err("SET expects at least 2 arguments".into())));
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
    fn test_rpush() {
        let storage = Storage::default();
        {   // 0 arguments provided
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("rpush".into())),
            ]));
            let response = handle_request(request, &storage);
            assert_eq!(response, Ok(Value::Err("USAGE: RPUSH <key> <vals...>".into())));
        }
        {   // wrongtype
            let request1 = Value::Arr(Some(vec![
                Value::BulkStr(Some("Set".into())),
                Value::BulkStr(Some("key".into())),
                Value::Int(1),
            ]));
            let response1 = handle_request(request1, &storage);
            assert_eq!(response1, Ok(Value::Str("OK".into())));

            let request2 = Value::Arr(Some(vec![
                Value::BulkStr(Some("Rpush".into())),
                Value::BulkStr(Some("key".into())),
                Value::Int(1),
            ]));
            let response2 = handle_request(request2, &storage);
            assert_eq!(response2, Ok(Value::Err("WRONGTYPE for list: Int(1)".into())));
        }
        {   // correct case
            let request1 = Value::Arr(Some(vec![
                Value::BulkStr(Some("rpush".into())),
                Value::BulkStr(Some("lst".into())),
                Value::Int(1),
            ]));
            let response1 = handle_request(request1, &storage);
            assert_eq!(response1, Ok(Value::Int(1)));

            let request2 = Value::Arr(Some(vec![
                Value::BulkStr(Some("Rpush".into())),
                Value::BulkStr(Some("lst".into())),
                Value::Str("two".into()),
            ]));
            let response2 = handle_request(request2, &storage);
            assert_eq!(response2, Ok(Value::Int(2)));

            let request3 = Value::Arr(Some(vec![
                Value::BulkStr(Some("Get".into())),
                Value::BulkStr(Some("lst".into())),
            ]));
            let response3 = handle_request(request3, &storage);
            assert_eq!(
                response3,
                Ok(Value::Arr(Some(vec![Value::Int(1), Value::Str("two".into())])))
            );
        }
        {   // correct case : multiple vals
            let request1 = Value::Arr(Some(vec![
                Value::BulkStr(Some("rpush".into())),
                Value::BulkStr(Some("key10".into())),
                Value::Int(1), Value::Str("2".into()), Value::Int(3) 
            ]));
            let response1 = handle_request(request1, &storage);
            assert_eq!(response1, Ok(Value::Int(3)));

            let request2 = Value::Arr(Some(vec![
                Value::BulkStr(Some("rpush".into())),
                Value::BulkStr(Some("key10".into())),
                Value::Int(4), Value::Int(5) 
            ]));
            let response2 = handle_request(request2, &storage);
            assert_eq!(response2, Ok(Value::Int(5)));

            let request3 = Value::Arr(Some(vec![
                Value::BulkStr(Some("Get".into())),
                Value::BulkStr(Some("key10".into())),
            ]));
            let response3 = handle_request(request3, &storage);
            assert_eq!(
                response3,
                Ok(Value::Arr(Some(vec![
                    Value::Int(1), Value::Str("2".into()), Value::Int(3), Value::Int(4), Value::Int(5)
                ])))
            );
        }
    }

    #[test]
    fn test_lrange() {
        let storage = Storage::default();
        storage.lock().unwrap().insert(
            b"key1".into(),
            Value::Arr(Some(vec![
                Value::Int(1), Value::Str("2".into()), Value::Int(3), Value::Int(4), Value::Int(5)
            ])).into()
        );
        {
            let request = Value::Arr(Some(vec![
                Value::BulkStr(Some("lrange".into())),
                Value::BulkStr(Some("key1".into())),
                Value::BulkStr(Some("1".into())),
                Value::BulkStr(Some("3".into())),
            ]));
            let response = handle_request(request, &storage);
            assert_eq!(
                response,
                Ok(Value::Arr(Some(vec![
                    Value::Str("2".into()), Value::Int(3), Value::Int(4)
                ])))
            );
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
