//! Vision boxes use a bottom-left origin; rows below use top-to-bottom order.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OcrBlock {
    pub text: String,
    pub confidence: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OcrLayout {
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
    pub blocks: Vec<OcrBlock>,
}
pub fn table(layout: &OcrLayout) -> Option<String> {
    if layout.orientation != 1
        || layout.blocks.len() < 4
        || layout.blocks.len() > 2000
        || layout.blocks.iter().any(|b| {
            b.confidence < 0.65
                || b.text.contains(['|', '\n'])
                || ![b.x, b.y, b.width, b.height]
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                || b.height <= 0.0
        })
    {
        return None;
    }
    let mut blocks: Vec<_> = layout.blocks.iter().collect();
    blocks.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let mut rows: Vec<Vec<&OcrBlock>> = vec![];
    for block in blocks {
        if let Some(row) = rows.last_mut()
            && (row[0].y - block.y).abs() < row[0].height.min(block.height) * 0.6
        {
            row.push(block);
        } else {
            rows.push(vec![block]);
        }
    }
    for row in &mut rows {
        row.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    let columns = rows.iter().max_by_key(|r| r.len())?;
    if !(2..=12).contains(&columns.len()) || rows.len() < 2 {
        return None;
    }
    let edges: Vec<_> = columns.iter().map(|b| b.x).collect();
    let mut result = vec![];
    let mut full_rows = 0;
    for row in rows {
        let mut cells = vec![String::new(); edges.len()];
        let count = row.len();
        for block in row {
            let col = edges.iter().position(|x| (*x - block.x).abs() < 0.025)?;
            if !cells[col].is_empty() {
                return None;
            }
            if col + 1 < edges.len() && block.x + block.width > edges[col + 1] + 0.025 {
                return None;
            }
            cells[col] = block.text.clone();
        }
        full_rows += usize::from(count == edges.len());
        result.push(format!("| {} |", cells.join(" | ")));
    }
    if full_rows < 2 {
        return None;
    }
    result.insert(1, format!("| {} |", vec!["---"; edges.len()].join(" | ")));
    Some(result.join("\n"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rows_preserve_empty_cells_and_bottom_left_order() {
        let blocks = [
            ("A", 0.1, 0.8),
            ("B", 0.6, 0.8),
            ("1", 0.1, 0.6),
            ("2", 0.6, 0.6),
            ("3", 0.6, 0.4),
        ]
        .into_iter()
        .map(|(t, x, y)| OcrBlock {
            text: t.into(),
            confidence: 0.9,
            x,
            y,
            width: 0.1,
            height: 0.05,
        })
        .collect();
        let mut layout = OcrLayout {
            width: 1000,
            height: 1000,
            orientation: 1,
            blocks,
        };
        assert_eq!(
            table(&layout).unwrap(),
            "| A | B |\n| --- | --- |\n| 1 | 2 |\n|  | 3 |"
        );
        layout.blocks[0].confidence = 0.2;
        assert!(table(&layout).is_none());
    }
}
