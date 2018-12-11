use std::cmp::max;

use text_ui::formatter::{LineFormatter, RoundingBehavior};
use ropey::RopeSlice;
use text_ui::string_utils::{rope_slice_is_line_ending, rope_slice_is_whitespace};
use text_ui::utils::grapheme_width;

pub enum WrapType {
    NoWrap,
    CharWrap(usize),
    WordWrap(usize),
}

// ===================================================================
// LineFormatter implementation for terminals/consoles.
// ===================================================================

pub struct ConsoleLineFormatter {
    pub tab_width: u8,
    pub wrap_type: WrapType,
    pub maintain_indent: bool,
    pub wrap_additional_indent: usize,
}

impl ConsoleLineFormatter {
    pub fn new(tab_width: u8) -> ConsoleLineFormatter {
        ConsoleLineFormatter {
            tab_width: tab_width,
            wrap_type: WrapType::WordWrap(40),
            maintain_indent: true,
            wrap_additional_indent: 0,
        }
    }

    pub fn set_wrap_width(&mut self, width: usize) {
        match self.wrap_type {
            WrapType::NoWrap => {}

            WrapType::CharWrap(ref mut w) => {
                *w = width;
            }

            WrapType::WordWrap(ref mut w) => {
                *w = width;
            }
        }
    }

    pub fn iter<'a, T>(&'a self, g_iter: T) -> ConsoleLineFormatterVisIter<'a, T>
    where
        T: Iterator<Item = RopeSlice<'a>>,
    {
        ConsoleLineFormatterVisIter::<'a, T> {
            grapheme_iter: g_iter,
            f: self,
            pos: (0, 0),
            indent: 0,
            indent_found: false,
            word_buf: Vec::new(),
            word_i: 0,
        }
    }
}

impl LineFormatter for ConsoleLineFormatter {
    fn single_line_height(&self) -> usize {
        return 1;
    }

    fn dimensions<'a, T>(&'a self, g_iter: T) -> (usize, usize)
    where
        T: Iterator<Item = RopeSlice<'a>>,
    {
        let mut dim: (usize, usize) = (0, 0);

        for (_, pos, width) in self.iter(g_iter) {
            dim = (max(dim.0, pos.0), max(dim.1, pos.1 + width));
        }

        dim.0 += self.single_line_height();

        return dim;
    }

    fn index_to_v2d<'a, T>(&'a self, g_iter: T, char_idx: usize) -> (usize, usize)
    where
        T: Iterator<Item = RopeSlice<'a>>,
    {
        let mut pos = (0, 0);
        let mut i = 0;
        let mut last_width = 0;

        for (g, _pos, width) in self.iter(g_iter) {
            pos = _pos;
            last_width = width;
            i += g.chars().count();

            if i > char_idx {
                return pos;
            }
        }

        return (pos.0, pos.1 + last_width);
    }

    fn v2d_to_index<'a, T>(
        &'a self,
        g_iter: T,
        v2d: (usize, usize),
        _: (RoundingBehavior, RoundingBehavior),
    ) -> usize
    where
        T: Iterator<Item = RopeSlice<'a>>,
    {
        // TODO: handle rounding modes
        let mut prev_i = 0;
        let mut i = 0;

        for (g, pos, _) in self.iter(g_iter) {
            if pos.0 > v2d.0 {
                i = prev_i;
                break;
            } else if pos.0 == v2d.0 && pos.1 >= v2d.1 {
                break;
            }

            prev_i = i;
            i += g.chars().count();
        }

        return i;
    }
}

// ===================================================================
// An iterator that iterates over the graphemes in a line in a
// manner consistent with the ConsoleFormatter.
// ===================================================================
pub struct ConsoleLineFormatterVisIter<'a, T>
where
    T: Iterator<Item = RopeSlice<'a>>,
{
    grapheme_iter: T,
    f: &'a ConsoleLineFormatter,
    pos: (usize, usize),

    indent: usize,
    indent_found: bool,

    word_buf: Vec<RopeSlice<'a>>,
    word_i: usize,
}

impl<'a, T> ConsoleLineFormatterVisIter<'a, T>
where
    T: Iterator<Item = RopeSlice<'a>>,
{
    fn next_nowrap(&mut self, g: RopeSlice<'a>) -> Option<(RopeSlice<'a>, (usize, usize), usize)> {
        let width = grapheme_vis_width_at_vis_pos(g, self.pos.1, self.f.tab_width as usize);

        let pos = self.pos;
        self.pos = (self.pos.0, self.pos.1 + width);
        return Some((g, pos, width));
    }

    fn next_charwrap(
        &mut self,
        g: RopeSlice<'a>,
        wrap_width: usize,
    ) -> Option<(RopeSlice<'a>, (usize, usize), usize)> {
        let width = grapheme_vis_width_at_vis_pos(g, self.pos.1, self.f.tab_width as usize);

        if (self.pos.1 + width) > wrap_width {
            if !self.indent_found {
                self.indent = 0;
                self.indent_found = true;
            }

            if self.f.maintain_indent {
                let pos = (
                    self.pos.0 + self.f.single_line_height(),
                    self.indent + self.f.wrap_additional_indent,
                );
                self.pos = (
                    self.pos.0 + self.f.single_line_height(),
                    self.indent + self.f.wrap_additional_indent + width,
                );
                return Some((g, pos, width));
            } else {
                let pos = (
                    self.pos.0 + self.f.single_line_height(),
                    self.f.wrap_additional_indent,
                );
                self.pos = (
                    self.pos.0 + self.f.single_line_height(),
                    self.f.wrap_additional_indent + width,
                );
                return Some((g, pos, width));
            }
        } else {
            if !self.indent_found {
                if rope_slice_is_whitespace(&g) {
                    self.indent += width;
                } else {
                    self.indent_found = true;
                }
            }

            let pos = self.pos;
            self.pos = (self.pos.0, self.pos.1 + width);
            return Some((g, pos, width));
        }
    }
}

impl<'a, T> Iterator for ConsoleLineFormatterVisIter<'a, T>
where
    T: Iterator<Item = RopeSlice<'a>>,
{
    type Item = (RopeSlice<'a>, (usize, usize), usize);

    fn next(&mut self) -> Option<(RopeSlice<'a>, (usize, usize), usize)> {
        match self.f.wrap_type {
            WrapType::NoWrap => {
                if let Some(g) = self.grapheme_iter.next() {
                    return self.next_nowrap(g);
                } else {
                    return None;
                }
            }

            WrapType::CharWrap(wrap_width) => {
                if let Some(g) = self.grapheme_iter.next() {
                    return self.next_charwrap(g, wrap_width);
                } else {
                    return None;
                }
            }

            WrapType::WordWrap(wrap_width) => {
                // Get next word if necessary
                if self.word_i >= self.word_buf.len() {
                    let mut word_width = 0;
                    self.word_buf.truncate(0);
                    while let Some(g) = self.grapheme_iter.next() {
                        self.word_buf.push(g);
                        let width = grapheme_vis_width_at_vis_pos(
                            g,
                            self.pos.1 + word_width,
                            self.f.tab_width as usize,
                        );
                        word_width += width;
                        if rope_slice_is_whitespace(&g) {
                            break;
                        }
                    }

                    if self.word_buf.len() == 0 {
                        return None;
                    } else if !self.indent_found && !rope_slice_is_whitespace(&self.word_buf[0]) {
                        self.indent_found = true;
                    }

                    // Move to next line if necessary
                    if (self.pos.1 + word_width) > wrap_width {
                        if !self.indent_found {
                            self.indent = 0;
                            self.indent_found = true;
                        }

                        if self.pos.1 > 0 {
                            if self.f.maintain_indent {
                                self.pos = (
                                    self.pos.0 + self.f.single_line_height(),
                                    self.indent + self.f.wrap_additional_indent,
                                );
                            } else {
                                self.pos = (
                                    self.pos.0 + self.f.single_line_height(),
                                    self.f.wrap_additional_indent,
                                );
                            }
                        }
                    }

                    self.word_i = 0;
                }

                // Iterate over the word
                let g = self.word_buf[self.word_i];
                self.word_i += 1;
                return self.next_charwrap(g, wrap_width);
            }
        }
    }
}

// ===================================================================
// Helper functions
// =======================x============================================

/// Returns the visual width of a grapheme given a starting
/// position on a line.
fn grapheme_vis_width_at_vis_pos(g: RopeSlice, pos: usize, tab_width: usize) -> usize {
    if g == "\t" {
        let ending_pos = ((pos / tab_width) + 1) * tab_width;
        return ending_pos - pos;
    } else if rope_slice_is_line_ending(&g) {
        return 1;
    } else {
        return grapheme_width(&g);
    }
}
