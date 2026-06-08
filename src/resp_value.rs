use std::io;
use crate::resp_parser::{ArrParser, BulkStrParser, ErrParser, IntParser, Parser, StrParser};

type Bytes = Vec<u8>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Err(String),
    Int(i64),
    BulkStr(Option<Bytes>),
    Arr(Option<Vec<Value>>),
}

impl Value {
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        match self {
            Value::Str(s) => Self::serialize_str(writer, &s)?,
            Value::Err(s) => Self::serialize_err(writer, &s)?,
            Value::Int(num) => Self::serialize_int(writer, *num)?,
            Value::BulkStr(s) => Self::serialize_bulk_str(writer, &s)?,
            Value::Arr(arr) => Self::serialize_arr(writer, arr)?,
        }
        Ok(())
    }

    fn serialize_str<W: io::Write>(writer: &mut W, s: &String) -> io::Result<()> {
        writer.write_all(&[StrParser::STARTING_BYTE])?;
        writer.write_all(s.as_bytes())?;
        writer.write_all(b"\r\n")?;
        Ok(())
    }

    fn serialize_err<W: io::Write>(writer: &mut W, s: &String) -> io::Result<()> {
        writer.write_all(&[ErrParser::STARTING_BYTE])?;
        writer.write_all(s.as_bytes())?;
        writer.write_all(b"\r\n")?;
        Ok(())
    }

    fn serialize_int<W: io::Write>(writer: &mut W, num: i64) -> io::Result<()> {
        writer.write_all(&[IntParser::STARTING_BYTE])?;
        writer.write_all(num.to_string().as_bytes())?;
        writer.write_all(b"\r\n")?;
        Ok(())
    }

    fn serialize_bulk_str<W: io::Write>(writer: &mut W, s: &Option<Bytes>) -> io::Result<()> {
        writer.write_all(&[BulkStrParser::STARTING_BYTE])?;
        match s {
            Some(s) => {
                writer.write_all(s.len().to_string().as_bytes())?;
                writer.write_all(b"\r\n")?;
                writer.write_all(s)?;
            },
            None => {
                writer.write_all(b"-1")?;
            }
        }
        writer.write_all(b"\r\n")?;
        Ok(())
    }

    fn serialize_arr<W: io::Write>(writer: &mut W, arr: &Option<Vec<Value>>) -> io::Result<()> {
        writer.write_all(&[ArrParser::STARTING_BYTE])?;
        match arr {
            Some(arr) => {
                writer.write_all(arr.len().to_string().as_bytes())?;
                writer.write_all(b"\r\n")?;
                for val in arr {
                    val.serialize(writer)?;
                }
            },
            None => {
                writer.write_all(b"-1")?;
                writer.write_all(b"\r\n")?;
            }
        }
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use io::{Cursor, BufRead, Seek};

    fn serialize_and_rewind(value: Value) -> Cursor<Vec<u8>> {
        let mut cursor = Cursor::new(vec![]);
        let result = value.serialize(&mut cursor);
        assert!(result.is_ok());
        cursor.rewind().unwrap();
        cursor
    }

    #[test]
    fn test_serialize_str() {
        {   // empty str
            let value = Value::Str("".into());
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"+\r\n");
        }
        {   // non-empty str
            let value = Value::Str("hello".into());
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"+hello\r\n");
        }        
    }

    #[test]
    fn test_serialize_err() {
        {   // empty str
            let value = Value::Err("".into());
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"-\r\n");
        }
        {   // non-empty str
            let value = Value::Err("error".into());
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"-error\r\n");
        }        
    }

    #[test]
    fn test_serialize_int() {
        {   // just int
            let value = Value::Int(123);
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b":123\r\n");
        }
    }    

    #[test]
    fn test_serialize_bulk_str() {
        {   // null str
            let value = Value::BulkStr(None);
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"$-1\r\n");
        }
        {   // empty str
            let value = Value::BulkStr(Some("".into()));
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"$0\r\n\r\n");
        }
        {   // non-empty str
            let value = Value::BulkStr(Some("correct\r\ncase\n\r".into()));
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"$15\r\ncorrect\r\ncase\n\r\r\n");
        }    
    }    

    #[test]
    fn test_serialize_arr() {
        {   // null arr
            let value = Value::Arr(None);
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"*-1\r\n");
        }
        {   // empty arr
            let value = Value::Arr(Some(vec![]));
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"*0\r\n");
        }
        {   // non-empty case 1 [BulkStr, Str, Int]
            let value = Value::Arr(Some(vec![
                Value::BulkStr(Some("some".into())),
                Value::Str("number".into()),
                Value::Int(321),
            ]));
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"*3\r\n$4\r\nsome\r\n+number\r\n:321\r\n");
        }           
        {   // non-empty case 2 [[Int, Int, Int], [Str, Str], [Err]]
            let value = Value::Arr(Some(vec![
                Value::Arr(Some(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
                Value::Arr(Some(vec![Value::Str("Hello".into()), Value::Str("World".into())])),
                Value::Arr(Some(vec![Value::Err("some error".into())])),
            ]));
            let mut cursor = serialize_and_rewind(value);
            assert_eq!(cursor.fill_buf().unwrap(), b"*3\r\n*3\r\n:1\r\n:2\r\n:3\r\n*2\r\n+Hello\r\n+World\r\n*1\r\n-some error\r\n");
        }    
    }        
}