//! File I/O commands: FI, FO, FE, FK, FB, FS, FT, FX, FGI, FGO, FGR, FGW, FGB, FGK.

use std::fmt::Write;

use crate::file_io;
use crate::{
    cmd_result::CmdFailure, cmd_result::CmdResult, lead_param::LeadParam, marks::MarkId,
    position::Position, trail_param::TrailParam,
};

use super::ExecutionContext;

impl ExecutionContext<'_> {
    /// FI — File Input
    ///
    /// `FI/path/` — open a file and load its content into the current frame.
    /// `-FI//`    — close the input file association.
    pub(crate) fn cmd_file_input(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                if self.frame_set.current_frame().input_file.is_none() {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }
                self.frame_set.current_frame_mut().input_file = None;
                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                if self.frame_set.current_frame().input_file.is_some() {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = std::path::PathBuf::from(&path_str);

                let Ok(mut fh) = file_io::open_input(&path) else {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                };

                // Read all content (handle not yet stored on frame — no borrow conflict).
                let text = file_io::read_all(&mut fh);

                // Clear frame and load file content.
                let frame = self.frame_set.current_frame_mut();
                frame.clear_content();
                if !text.is_empty() {
                    frame.insert_at(Position::zero(), &text);
                }
                frame.set_dot(Position::zero());
                frame.input_file = Some(fh);

                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FO — File Output
    ///
    /// `FO/path/` — open an output file (writes frame content when closed).
    /// `-FO//`    — write all frame content to the output file, finalize, close.
    pub(crate) fn cmd_file_output(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                if self.frame_set.current_frame().output_file.is_none() {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }

                // Collect frame content and modified flag before taking the handle.
                let text = self.frame_set.current_frame().text();
                let modified = self
                    .frame_set
                    .current_frame()
                    .get_mark(MarkId::Modified)
                    .is_some();

                let mut fh = self
                    .frame_set
                    .current_frame_mut()
                    .output_file
                    .take()
                    .unwrap();

                if file_io::write_all(&mut fh, &text).is_err() {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                }
                if file_io::finalize_output(&mut fh, modified).is_err() {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                }

                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                if self.frame_set.current_frame().output_file.is_some() {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = if path_str.is_empty() {
                    // Fall back to the input file's path.
                    match self.frame_set.current_frame().input_file.as_ref() {
                        Some(fh) => fh.path.clone(),
                        None => return CmdResult::Failure(CmdFailure::SyntaxError),
                    }
                } else {
                    std::path::PathBuf::from(&path_str)
                };

                let Ok(fh) = file_io::open_output(&path, false, 1, false) else {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                };

                self.frame_set.current_frame_mut().output_file = Some(fh);
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FE — File Edit
    ///
    /// `FE/path/` — open both input and output on the same file.
    /// `-FE//`    — write output, finalize, close both handles.
    pub(crate) fn cmd_file_edit(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                let has_input = self.frame_set.current_frame().input_file.is_some();
                let has_output = self.frame_set.current_frame().output_file.is_some();

                if !has_input && !has_output {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }

                // Finalize output if open.
                if has_output {
                    let text = self.frame_set.current_frame().text();
                    let modified = self
                        .frame_set
                        .current_frame()
                        .get_mark(MarkId::Modified)
                        .is_some();
                    let mut fh = self
                        .frame_set
                        .current_frame_mut()
                        .output_file
                        .take()
                        .unwrap();

                    if file_io::write_all(&mut fh, &text).is_err() {
                        return CmdResult::Failure(CmdFailure::FileOpenError);
                    }
                    if file_io::finalize_output(&mut fh, modified).is_err() {
                        return CmdResult::Failure(CmdFailure::FileOpenError);
                    }
                }

                // Close input.
                self.frame_set.current_frame_mut().input_file = None;

                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                let has_input = self.frame_set.current_frame().input_file.is_some();
                let has_output = self.frame_set.current_frame().output_file.is_some();

                if has_input || has_output {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = std::path::PathBuf::from(&path_str);

                // Open input and read all content.
                let Ok(mut input_fh) = file_io::open_input(&path) else {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                };
                let text = file_io::read_all(&mut input_fh);

                // Open output.
                let Ok(output_fh) = file_io::open_output(&path, false, 1, false) else {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                };

                // Load content into frame.
                let frame = self.frame_set.current_frame_mut();
                frame.clear_content();
                if !text.is_empty() {
                    frame.insert_at(Position::zero(), &text);
                }
                frame.set_dot(Position::zero());
                frame.input_file = Some(input_fh);
                frame.output_file = Some(output_fh);

                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FK — File Kill
    ///
    /// Deletes the per-frame output temp file without creating the real file.
    pub(crate) fn cmd_file_kill(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.current_frame().output_file.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let fh = self
            .frame_set
            .current_frame_mut()
            .output_file
            .take()
            .unwrap();
        file_io::delete_temp(&fh);

        CmdResult::Success
    }

    /// FB — File Back (rewind)
    ///
    /// Rewinds the per-frame input file and reloads the frame from the beginning.
    pub(crate) fn cmd_file_rewind(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.current_frame().input_file.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        // Take the handle out to allow mutable access to the frame simultaneously.
        let mut fh = self
            .frame_set
            .current_frame_mut()
            .input_file
            .take()
            .unwrap();

        if file_io::rewind(&mut fh).is_err() {
            // Put it back on error so the caller can still clean up.
            self.frame_set.current_frame_mut().input_file = Some(fh);
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }

        let text = file_io::read_all(&mut fh);

        let frame = self.frame_set.current_frame_mut();
        frame.clear_content();
        if !text.is_empty() {
            frame.insert_at(Position::zero(), &text);
        }
        frame.set_dot(Position::zero());
        frame.input_file = Some(fh);

        CmdResult::Success
    }

    /// FS — File Save
    ///
    /// Saves the current frame to its output file (if modified), then reopens
    /// both handles for continued editing.  No-op if the frame is unmodified.
    pub(crate) fn cmd_file_save(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.current_frame().output_file.is_none() {
            return CmdResult::Failure(CmdFailure::NoOutputFile);
        }

        // No-op if not modified.
        if self
            .frame_set
            .current_frame()
            .get_mark(MarkId::Modified)
            .is_none()
        {
            return CmdResult::Success;
        }

        // Collect frame text, then take the output handle.
        let text = self.frame_set.current_frame().text();
        let mut out = self
            .frame_set
            .current_frame_mut()
            .output_file
            .take()
            .unwrap();

        if file_io::write_all(&mut out, &text).is_err() {
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }

        // Remember path parameters before finalize consumes them.
        let path = out.path.clone();
        let entab = out.entab;
        let versions = out.versions;
        let purge = out.purge;

        if file_io::finalize_output(&mut out, true).is_err() {
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }

        // Reopen for continued editing.
        let Ok(mut new_input) = file_io::open_input(&path) else {
            return CmdResult::Failure(CmdFailure::FileOpenError);
        };
        let new_text = file_io::read_all(&mut new_input);

        let Ok(new_output) = file_io::open_output(&path, entab, versions, purge) else {
            return CmdResult::Failure(CmdFailure::FileOpenError);
        };

        let frame = self.frame_set.current_frame_mut();
        frame.clear_content();
        if !new_text.is_empty() {
            frame.insert_at(Position::zero(), &new_text);
        }
        frame.set_dot(Position::zero());
        frame.input_file = Some(new_input);
        frame.output_file = Some(new_output);

        CmdResult::Success
    }

    /// FT — File Table
    ///
    /// Displays the file table (no-op in batch mode).
    pub(crate) fn cmd_file_table(&mut self, _lead: LeadParam) -> CmdResult {
        // No-op in batch mode; interactive display deferred to Phase 10.
        CmdResult::Success
    }

    /// FGI — Global File Input
    ///
    /// `FGI/path/` — open global input file.
    /// `-FGI//`    — close global input file.
    pub(crate) fn cmd_fglobal_input(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                if self.frame_set.global_input.is_none() {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }
                self.frame_set.global_input = None;
                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                if self.frame_set.global_input.is_some() {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = std::path::PathBuf::from(&path_str);

                let Ok(fh) = file_io::open_input(&path) else {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                };

                self.frame_set.global_input = Some(fh);
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FGO — Global File Output
    ///
    /// `FGO/path/` — open global output file.
    /// `-FGO//`    — flush, finalize, close global output file.
    pub(crate) fn cmd_fglobal_output(
        &mut self,
        lead: LeadParam,
        tpars: &[TrailParam],
    ) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                if self.frame_set.global_output.is_none() {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }

                let mut fh = self.frame_set.global_output.take().unwrap();
                // Global output: no backups by default.
                if file_io::finalize_output(&mut fh, false).is_err() {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                }

                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                if self.frame_set.global_output.is_some() {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = std::path::PathBuf::from(&path_str);

                let Ok(fh) = file_io::open_output(&path, false, 0, false) else {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                };

                self.frame_set.global_output = Some(fh);
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FGR — Global File Read
    ///
    /// Reads N lines from the global input file and inserts them at the current dot.
    pub(crate) fn cmd_file_read(&mut self, lead: LeadParam) -> CmdResult {
        if self.frame_set.global_input.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let count = match lead {
            LeadParam::None | LeadParam::Plus => 1usize,
            LeadParam::Pint(n) => n,
            LeadParam::Pindef => usize::MAX,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Take the handle out to read from it, then put it back.
        let mut fh = self.frame_set.global_input.take().unwrap();
        let lines = file_io::read_lines(&mut fh, count);
        let at_eof = fh.at_eof;
        self.frame_set.global_input = Some(fh);

        if lines.is_empty() && at_eof {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }

        if lines.is_empty() {
            return CmdResult::Success;
        }

        // Build insertion text.
        let text = lines.iter().fold(String::new(), |mut output, l| {
            let _ = writeln!(output, "{l}");
            output
        });

        let dot = self.frame_set.current_frame().dot();
        self.frame_set
            .current_frame_mut()
            .set_mark_at(MarkId::Equals, dot);
        self.frame_set.current_frame_mut().insert_at(dot, &text);

        let new_dot = self.frame_set.current_frame().dot();
        self.frame_set
            .current_frame_mut()
            .set_mark_at(MarkId::Modified, new_dot);

        CmdResult::Success
    }

    /// FGW — Global File Write
    ///
    /// Writes N lines from the current frame to the global output file.
    pub(crate) fn cmd_file_write(&mut self, lead: LeadParam) -> CmdResult {
        if self.frame_set.global_output.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let count = match lead {
            LeadParam::None | LeadParam::Plus => 1usize,
            LeadParam::Pint(n) => n,
            LeadParam::Pindef => usize::MAX,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        let dot_line = self.frame_set.current_frame().dot().line;
        let line_count = self.frame_set.current_frame().line_count();
        // Exclude the null (EOF) line.
        let content_lines = line_count.saturating_sub(1);

        let end_line = if count == usize::MAX {
            content_lines
        } else {
            (dot_line + count).min(content_lines)
        };

        // Collect lines to write.
        let mut lines = Vec::new();
        for line_idx in dot_line..end_line {
            let content = self
                .frame_set
                .current_frame()
                .line_content(line_idx)
                .map(|s| {
                    let s = s.to_string();
                    s.trim_end_matches('\n').trim_end_matches('\r').to_string()
                })
                .unwrap_or_default();
            lines.push(content);
        }

        // Write lines.
        let mut fh = self.frame_set.global_output.take().unwrap();
        if file_io::write_lines(&mut fh, &lines).is_err() {
            self.frame_set.global_output = Some(fh);
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }
        self.frame_set.global_output = Some(fh);

        // Advance dot to the line after the last written.
        let new_line = end_line.min(content_lines);
        self.frame_set
            .current_frame_mut()
            .set_dot(Position::new(new_line, 0));

        CmdResult::Success
    }

    /// FGB — Global File Back (rewind)
    ///
    /// Rewinds the global input file without affecting any frame.
    pub(crate) fn cmd_fglobal_rewind(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.global_input.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let mut fh = self.frame_set.global_input.take().unwrap();
        if file_io::rewind(&mut fh).is_err() {
            self.frame_set.global_input = Some(fh);
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }
        self.frame_set.global_input = Some(fh);

        CmdResult::Success
    }

    /// FGK — Global File Kill
    ///
    /// Deletes the global output temp file without creating the real file.
    pub(crate) fn cmd_fglobal_kill(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.global_output.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let fh = self.frame_set.global_output.take().unwrap();
        file_io::delete_temp(&fh);

        CmdResult::Success
    }
}
