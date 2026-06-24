use crate::Bytes;
use crate::resp_parser::{ArrParser, BulkStrParser, ErrParser, IntParser, Parser, StrParser};

use std::io;
use std::marker::Unpin;
use tokio::io::AsyncWriteExt;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Err(String),
    Int(i64),
    BulkStr(Option<Bytes>),
    Arr(Option<Vec<Value>>),
}

impl Default for Value {
    fn default() -> Self {
        Self::BulkStr(None)
    }
}

impl Value {
    pub async fn serialize_to_stream<W: AsyncWriteExt + Unpin>(&self, writer: &mut W) -> io::Result<()> {
        let mut buf = Vec::new();
        self.serialize(&mut buf);
        writer.write_all(buf.as_slice()).await?;
        Ok(())
    }

    pub fn serialize(&self, buf: &mut Bytes) {
        match self {
            Value::Str(s) => Self::serialize_str(buf, &s),
            Value::Err(s) => Self::serialize_err(buf, &s),
            Value::Int(num) => Self::serialize_int(buf, *num),
            Value::BulkStr(s) => Self::serialize_bulk_str(buf, &s),
            Value::Arr(arr) => Self::serialize_arr(buf, arr),
        }
    }

    fn serialize_str(buf: &mut Bytes, s: &String) {
        buf.push(StrParser::STARTING_BYTE);
        buf.extend_from_slice(s.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    fn serialize_err(buf: &mut Bytes, s: &String) {
        buf.push(ErrParser::STARTING_BYTE);
        buf.extend_from_slice(s.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    fn serialize_int(buf: &mut Bytes, num: i64) {
        buf.push(IntParser::STARTING_BYTE);
        buf.extend_from_slice(num.to_string().as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    fn serialize_bulk_str(buf: &mut Bytes, s: &Option<Bytes>) {
        buf.push(BulkStrParser::STARTING_BYTE);
        match s {
            Some(s) => {
                buf.extend_from_slice(s.len().to_string().as_bytes());
                buf.extend_from_slice(b"\r\n");
                buf.extend_from_slice(s);
            },
            None => {
                buf.extend_from_slice(b"-1");
            }
        }
        buf.extend_from_slice(b"\r\n");
    }

    fn serialize_arr(buf: &mut Bytes, arr: &Option<Vec<Value>>) {
        buf.push(ArrParser::STARTING_BYTE);
        match arr {
            Some(arr) => {
                buf.extend_from_slice(arr.len().to_string().as_bytes());
                buf.extend_from_slice(b"\r\n");
                for val in arr {
                    val.serialize(buf);
                }
            },
            None => {
                buf.extend_from_slice(b"-1");
                buf.extend_from_slice(b"\r\n");
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use io::{Cursor, BufRead, Seek};

    async fn serialize_and_rewind(value: Value) -> Cursor<Bytes> {
        let mut cursor = Cursor::new(vec![]);
        let result = value.serialize_to_stream(&mut cursor).await;
        assert!(result.is_ok());
        cursor.rewind().unwrap();
        cursor
    }

    #[tokio::test]
    async fn test_serialize_str() {
        {   // empty str
            let value = Value::Str("".into());
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"+\r\n");
        }
        {   // non-empty str
            let value = Value::Str("hello".into());
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"+hello\r\n");
        }        
    }

    #[tokio::test]
    async fn test_serialize_err() {
        {   // empty str
            let value = Value::Err("".into());
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"-\r\n");
        }
        {   // non-empty str
            let value = Value::Err("error".into());
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"-error\r\n");
        }        
    }

    #[tokio::test]
    async fn test_serialize_int() {
        {   // just int
            let value = Value::Int(123);
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b":123\r\n");
        }
    }    

    #[tokio::test]
    async fn test_serialize_bulk_str() {
        {   // null str
            let value = Value::BulkStr(None);
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"$-1\r\n");
        }
        {   // empty str
            let value = Value::BulkStr(Some("".into()));
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"$0\r\n\r\n");
        }
        {   // non-empty str
            let value = Value::BulkStr(Some("correct\r\ncase\n\r".into()));
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"$15\r\ncorrect\r\ncase\n\r\r\n");
        }    
    }    

    #[tokio::test]
    async fn test_serialize_arr() {
        {   // null arr
            let value = Value::Arr(None);
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"*-1\r\n");
        }
        {   // empty arr
            let value = Value::Arr(Some(vec![]));
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"*0\r\n");
        }
        {   // non-empty case 1 [BulkStr, Str, Int]
            let value = Value::Arr(Some(vec![
                Value::BulkStr(Some("some".into())),
                Value::Str("number".into()),
                Value::Int(321),
            ]));
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"*3\r\n$4\r\nsome\r\n+number\r\n:321\r\n");
        }           
        {   // non-empty case 2 [[Int, Int, Int], [Str, Str], [Err]]
            let value = Value::Arr(Some(vec![
                Value::Arr(Some(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
                Value::Arr(Some(vec![Value::Str("Hello".into()), Value::Str("World".into())])),
                Value::Arr(Some(vec![Value::Err("some error".into())])),
            ]));
            let mut cursor = serialize_and_rewind(value).await;
            assert_eq!(cursor.fill_buf().unwrap(), b"*3\r\n*3\r\n:1\r\n:2\r\n:3\r\n*2\r\n+Hello\r\n+World\r\n*1\r\n-some error\r\n");
        }    
    }        
}