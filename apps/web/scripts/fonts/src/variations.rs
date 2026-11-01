use read_fonts::types::Tag;
use read_fonts::{FontRef, TableProvider};
use write_fonts::FontBuilder;

use crate::Result;

const EMBEDDED_PEAK_TUPLE: u16 = 0x8000;
const INTERMEDIATE_REGION: u16 = 0x4000;
const TUPLE_INDEX_MASK: u16 = 0x0FFF;
const SHARED_POINT_NUMBERS: u16 = 0x8000;
const TUPLE_COUNT_MASK: u16 = 0x0FFF;

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data.get(offset..offset + 2).ok_or("truncated table")?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_i16(data: &[u8], offset: usize) -> Result<i16> {
    Ok(read_u16(data, offset)? as i16)
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data.get(offset..offset + 4).ok_or("truncated table")?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_fixed(data: &[u8], offset: usize) -> Result<f64> {
    Ok(f64::from(read_u32(data, offset)? as i32) / 65536.0)
}

struct Axis {
    index: usize,
    count: usize,
    record: usize,
    default: f64,
}

fn find_axis(fvar: &[u8], tag: &[u8; 4]) -> Result<Option<(Axis, f64)>> {
    let axes_offset = usize::from(read_u16(fvar, 4)?);
    let count = usize::from(read_u16(fvar, 8)?);
    let axis_size = usize::from(read_u16(fvar, 10)?);
    for index in 0..count {
        let record = axes_offset + index * axis_size;
        if fvar.get(record..record + 4) == Some(tag.as_slice()) {
            let min = read_fixed(fvar, record + 4)?;
            let default = read_fixed(fvar, record + 8)?;
            return Ok(Some((Axis { index, count, record, default }, min)));
        }
    }
    Ok(None)
}

fn trim_fvar(fvar: &[u8], axis: &Axis, low: f64) -> Result<Vec<u8>> {
    let axes_offset = usize::from(read_u16(fvar, 4)?);
    let axis_size = usize::from(read_u16(fvar, 10)?);
    let instance_count = usize::from(read_u16(fvar, 12)?);
    let instance_size = usize::from(read_u16(fvar, 14)?);
    let instances_offset = axes_offset + axis.count * axis_size;
    let mut output = fvar[..instances_offset].to_vec();
    output[axis.record + 4..axis.record + 8]
        .copy_from_slice(&fvar[axis.record + 8..axis.record + 12]);
    let mut kept = 0u16;
    for instance in 0..instance_count {
        let start = instances_offset + instance * instance_size;
        if read_fixed(fvar, start + 4 + axis.index * 4)? >= low {
            output.extend_from_slice(
                fvar.get(start..start + instance_size).ok_or("truncated fvar")?,
            );
            kept += 1;
        }
    }
    output[12..14].copy_from_slice(&kept.to_be_bytes());
    Ok(output)
}

fn packed_points_len(data: &[u8]) -> Result<usize> {
    let first = *data.first().ok_or("truncated points")?;
    let (count, mut position) = match first {
        0 => return Ok(1),
        _ if first & 0x80 != 0 => {
            let low = *data.get(1).ok_or("truncated points")?;
            ((usize::from(first & 0x7F) << 8) | usize::from(low), 2)
        }
        _ => (usize::from(first), 1),
    };
    let mut seen = 0;
    while seen < count {
        let control = *data.get(position).ok_or("truncated points")?;
        let run = usize::from(control & 0x7F) + 1;
        position += 1 + run * if control & 0x80 != 0 { 2 } else { 1 };
        seen += run;
    }
    Ok(position)
}

fn trim_glyph(data: &[u8], axis: &Axis, shared_tuples: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let count_field = read_u16(data, 0)?;
    let data_offset = usize::from(read_u16(data, 2)?);
    let has_shared_points = count_field & SHARED_POINT_NUMBERS != 0;
    let mut headers = Vec::new();
    let mut header_position = 4;
    let mut data_position = data_offset;
    if has_shared_points {
        data_position += packed_points_len(&data[data_offset..])?;
    }
    let shared_points = &data[data_offset..data_position];
    let mut payloads = Vec::new();
    let mut kept_count = 0u16;
    for _ in 0..count_field & TUPLE_COUNT_MASK {
        let size = usize::from(read_u16(data, header_position)?);
        let index = read_u16(data, header_position + 2)?;
        let mut header_len = 4;
        let peak = match index & EMBEDDED_PEAK_TUPLE != 0 {
            true => {
                header_len += 2 * axis.count;
                read_i16(data, header_position + 4 + 2 * axis.index)?
            }
            false => {
                let shared = usize::from(index & TUPLE_INDEX_MASK);
                read_i16(shared_tuples, 2 * (shared * axis.count + axis.index))?
            }
        };
        if index & INTERMEDIATE_REGION != 0 {
            header_len += 4 * axis.count;
        }
        if peak >= 0 {
            headers
                .extend_from_slice(&data[header_position..header_position + header_len]);
            payloads.extend_from_slice(
                data.get(data_position..data_position + size)
                    .ok_or("truncated gvar")?,
            );
            kept_count += 1;
        }
        header_position += header_len;
        data_position += size;
    }
    if kept_count == 0 {
        return Ok(Vec::new());
    }
    let flags = count_field & !TUPLE_COUNT_MASK;
    let mut output =
        Vec::with_capacity(4 + headers.len() + shared_points.len() + payloads.len());
    output.extend_from_slice(&(flags | kept_count).to_be_bytes());
    output.extend_from_slice(&u16::try_from(4 + headers.len())?.to_be_bytes());
    output.extend_from_slice(&headers);
    output.extend_from_slice(shared_points);
    output.extend_from_slice(&payloads);
    Ok(output)
}

fn trim_gvar(gvar: &[u8], axis: &Axis) -> Result<Vec<u8>> {
    let axis_count = usize::from(read_u16(gvar, 4)?);
    if axis_count != axis.count {
        return Err("gvar and fvar disagree on the axis count".into());
    }
    let shared_count = usize::from(read_u16(gvar, 6)?);
    let shared_offset = read_u32(gvar, 8)? as usize;
    let glyph_count = usize::from(read_u16(gvar, 12)?);
    let long_offsets = read_u16(gvar, 14)? & 1 != 0;
    let array_offset = read_u32(gvar, 16)? as usize;
    let offset_at = |glyph: usize| -> Result<usize> {
        match long_offsets {
            true => Ok(read_u32(gvar, 20 + glyph * 4)? as usize),
            false => Ok(usize::from(read_u16(gvar, 20 + glyph * 2)?) * 2),
        }
    };
    let shared_tuples = gvar
        .get(shared_offset..shared_offset + shared_count * axis_count * 2)
        .ok_or("truncated shared tuples")?;
    let mut glyphs = Vec::with_capacity(glyph_count);
    for glyph in 0..glyph_count {
        let start = array_offset + offset_at(glyph)?;
        let end = array_offset + offset_at(glyph + 1)?;
        glyphs.push(trim_glyph(
            gvar.get(start..end).ok_or("truncated glyph data")?,
            axis,
            shared_tuples,
        )?);
    }
    let header_len = 20 + (glyph_count + 1) * 4;
    let new_shared_offset = header_len;
    let new_array_offset = new_shared_offset + shared_tuples.len();
    let mut output = Vec::new();
    output.extend_from_slice(&gvar[..8]);
    output.extend_from_slice(&u32::try_from(new_shared_offset)?.to_be_bytes());
    output.extend_from_slice(&gvar[12..14]);
    output.extend_from_slice(&(read_u16(gvar, 14)? | 1).to_be_bytes());
    output.extend_from_slice(&u32::try_from(new_array_offset)?.to_be_bytes());
    let mut offset = 0usize;
    for glyph in &glyphs {
        output.extend_from_slice(&u32::try_from(offset)?.to_be_bytes());
        offset += glyph.len();
    }
    output.extend_from_slice(&u32::try_from(offset)?.to_be_bytes());
    output.extend_from_slice(shared_tuples);
    for glyph in &glyphs {
        output.extend_from_slice(glyph);
    }
    Ok(output)
}

pub fn drop_below(sfnt: &[u8], tag: &[u8; 4], low: f64) -> Result<Vec<u8>> {
    let font = FontRef::new(sfnt)?;
    let fvar = font
        .data_for_tag(Tag::new(b"fvar"))
        .map(|data| data.as_bytes().to_vec());
    let gvar = font
        .data_for_tag(Tag::new(b"gvar"))
        .map(|data| data.as_bytes().to_vec());
    let (Some(fvar), Some(gvar)) = (fvar, gvar) else {
        return Ok(sfnt.to_vec());
    };
    let Some((axis, min)) = find_axis(&fvar, tag)? else {
        return Ok(sfnt.to_vec());
    };
    if min >= low || axis.default != low {
        return Ok(sfnt.to_vec());
    }
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        let table = record.tag();
        let data = match &table.to_be_bytes() {
            b"fvar" => trim_fvar(&fvar, &axis, low)?,
            b"gvar" => trim_gvar(&gvar, &axis)?,
            _ => font.data_for_tag(table).ok_or("missing table")?.as_bytes().to_vec(),
        };
        builder.add_raw(table, data);
    }
    Ok(builder.build())
}
