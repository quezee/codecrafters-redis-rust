use std::io;
use crate::resp_parser::{ArrParser, BulkStrParser, ErrParser, IntParser, Parser, StrParser};

#[derive(Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Str(String),
    Err(String),
    Int(i64),
    BulkStr(String),
    Arr(Vec<Value>),
}

impl Value {
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        match self {
            Value::Null => Self::serialize_null(writer)?,
            Value::Str(s) => Self::serialize_str(writer, &s)?,
            Value::Err(s) => Self::serialize_err(writer, &s)?,
            Value::Int(num) => Self::serialize_int(writer, *num)?,
            Value::BulkStr(s) => Self::serialize_bulk_str(writer, &s)?,
            Value::Arr(arr) => Self::serialize_arr(writer, arr)?,
        }
        writer.write_all(b"\r\n")?;
        Ok(())
    }

    fn serialize_null<W: io::Write>(writer: &mut W) -> io::Result<()> {
        writer.write_all(b"$-1")?;
        Ok(())
    }

    fn serialize_str<W: io::Write>(writer: &mut W, s: &String) -> io::Result<()> {
        writer.write_all(&[StrParser::STARTING_BYTE])?;
        writer.write_all(s.as_bytes())?;
        Ok(())
    }

    fn serialize_err<W: io::Write>(writer: &mut W, s: &String) -> io::Result<()> {
        writer.write_all(&[ErrParser::STARTING_BYTE])?;
        writer.write_all(s.as_bytes())?;
        Ok(())
    }

    fn serialize_int<W: io::Write>(writer: &mut W, num: i64) -> io::Result<()> {
        writer.write_all(&[IntParser::STARTING_BYTE])?;
        writer.write_all(num.to_string().as_bytes())?;
        Ok(())
    }

    fn serialize_bulk_str<W: io::Write>(writer: &mut W, s: &String) -> io::Result<()> {
        writer.write_all(&[BulkStrParser::STARTING_BYTE])?;
        writer.write_all(s.len().to_string().as_bytes())?;
        writer.write_all(b"\r\n")?;
        writer.write_all(s.as_bytes())?;
        Ok(())
    }

    fn serialize_arr<W: io::Write>(writer: &mut W, arr: &Vec<Value>) -> io::Result<()> {
        writer.write_all(&[ArrParser::STARTING_BYTE])?;
        writer.write_all(arr.len().to_string().as_bytes())?;
        writer.write_all(b"\r\n")?;
        for val in arr {
            val.serialize(writer)?;
        }
        Ok(())
    }
}
