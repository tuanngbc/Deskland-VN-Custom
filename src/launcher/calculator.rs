//! Spotlight arithmetic, ported from FluidWin v1.3.1 ricerca.h / Calcolo.
//! Copyright (c) 2026 Clooudy, MIT; see third_party/fluidwin/LICENSE.
use super::{fullscreen, Entry, HWND};
use windows_sys::Win32::{Foundation::GlobalFree, System::{DataExchange::*, Memory::*}};

pub(super) const KIND: &str = "Calculation";

struct Calc<'a> { bytes: &'a [u8], cursor: usize }
impl Calc<'_> {
    fn next(&mut self) -> Option<u8> {
        while self.bytes.get(self.cursor) == Some(&b' ') { self.cursor += 1; }
        self.bytes.get(self.cursor).copied()
    }
    fn sum(&mut self, level: u8) -> Option<f64> {
        if level >= 150 { return None; }
        let mut value = self.product(level + 1)?;
        while let Some(op @ (b'+' | b'-')) = self.next() {
            self.cursor += 1;
            let rhs = self.product(level + 1)?;
            value = if op == b'+' { value + rhs } else { value - rhs };
        }
        Some(value)
    }
    fn product(&mut self, level: u8) -> Option<f64> {
        let mut value = self.power(level)?;
        while let Some(op @ (b'*' | b'x' | b'/' | b'%')) = self.next() {
            self.cursor += 1;
            value = match op {
                b'%' => value / 100.0,
                b'/' => value / self.power(level)?,
                _ => value * self.power(level)?,
            };
        }
        Some(value)
    }
    fn power(&mut self, level: u8) -> Option<f64> {
        if level >= 150 { return None; }
        let base = self.unary(level + 1)?;
        if self.next() != Some(b'^') { return Some(base); }
        self.cursor += 1;
        Some(base.powf(self.power(level + 1)?))
    }
    fn unary(&mut self, level: u8) -> Option<f64> {
        if level >= 150 { return None; }
        match self.next()? {
            op @ (b'+' | b'-') => {
                self.cursor += 1;
                let value = self.unary(level + 1)?;
                Some(if op == b'-' { -value } else { value })
            }
            b'(' => {
                self.cursor += 1;
                let value = self.sum(level + 1)?;
                if self.next() != Some(b')') { return None; }
                self.cursor += 1;
                Some(value)
            }
            _ => {
                let start = self.cursor;
                while self.bytes.get(self.cursor).is_some_and(|b| b.is_ascii_digit() || matches!(*b,b'.'|b',')) {
                    self.cursor += 1;
                }
                // Query is checked for ASCII before parsing, and decimal commas
                // are independent of the Windows user or process locale.
                std::str::from_utf8(&self.bytes[start..self.cursor]).ok()?.replace(',', ".").parse().ok()
            }
        }
    }
}

fn evaluate(input: &str) -> Option<f64> {
    let bytes = input.as_bytes();
    let operator = |b: u8| b"+-*/^%x()".contains(&b);
    if bytes.len() > 200 || !bytes.iter().any(u8::is_ascii_digit)
        || !bytes.iter().copied().any(operator)
        || bytes.iter().any(|b| !(b.is_ascii_digit() || operator(*b) || b" ,.".contains(b))) { return None; }
    let mut parser = Calc { bytes, cursor: 0 };
    let result = parser.sum(0)?;
    (result.is_finite() && parser.next().is_none()).then_some(if result == 0.0 { 0.0 } else { result })
}

pub(super) fn entry(query: &str) -> Option<Entry> {
    let n = evaluate(query)?;
    let result = if n != 0.0 && (n.abs() >= 1e16 || n.abs() < 1e-8) {
        format!("{n:e}")
    } else { n.to_string() };
    Some(Entry { name: format!("= {result}"), path: Default::default(), kind: KIND, detail: String::new() })
}
pub(super) fn hint() -> &'static str {
    fullscreen::words("Enter để sao chép kết quả", "Enter to copy result", "按 Enter 复制结果")
}
pub(super) fn copy_error() -> &'static str {
    fullscreen::words("Không sao chép được. Thử lại sau.", "Could not copy. Try again.", "无法复制，请重试。")
}
pub(super) unsafe fn copy(hwnd: HWND, entry: &Entry) -> bool {
    let Some(value) = entry.name.strip_prefix("= ").filter(|_| entry.kind == KIND) else { return false; };
    let wide = crate::desktop::wide(value);
    let handle = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
    if handle.is_null() { return false; }
    let dst = GlobalLock(handle).cast::<u16>();
    if dst.is_null() { GlobalFree(handle); return false; }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
    GlobalUnlock(handle);
    if OpenClipboard(hwnd) == 0 { GlobalFree(handle); return false; }
    let transferred = EmptyClipboard() != 0 && !SetClipboardData(13, handle).is_null();
    CloseClipboard();
    if !transferred { GlobalFree(handle); }
    transferred
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence_percent_decimal_and_rejection() {
        for (expr, expected) in [("2 + 3 * 4",14.0),("2^3^2",512.0),("25% x (3,5+0,5)",1.0),
            ("(-2)^3",-8.0),("8/2/2",2.0)] {
            assert_eq!(evaluate(expr),Some(expected),"{expr}");
        }
        for expr in ["", "12", "4/0", "1.2.3 + 1", "2^99999", "(1+2", "calc.exe", "1 & 2", "1+2.exe"] {
            assert_eq!(evaluate(expr),None,"{expr}");
        }
        assert!(evaluate(&format!("{}1{}","(".repeat(80),")".repeat(80))).is_none());
        assert!(evaluate(&format!("{}1","-".repeat(1000))).is_none());
    }
}
