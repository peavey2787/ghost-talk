use crate::{BroadcastFrame, BroadcastSink};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Recording sink for independently decodable Ghost voice units.
///
/// Each frame is retained as its original encoded unit. `finish` remuxes those
/// units with FFmpeg's concat demuxer, so the resulting recording has one
/// continuous timeline without recapturing or re-encoding the microphone.
pub struct FileRecordingSink {
    id: String,
    output: PathBuf,
    segments: PathBuf,
    input_format: String,
    count: u64,
    preserve_segments: bool,
}

impl FileRecordingSink {
    pub fn create(id: impl Into<String>, path: &Path, input_format: &str) -> Result<Self, String> {
        let segments = path.with_extension(format!("{input_format}.segments"));
        if segments.exists() {
            fs::remove_dir_all(&segments)
                .map_err(|error| format!("clear recording segments: {error}"))?;
        }
        fs::create_dir_all(&segments)
            .map_err(|error| format!("create recording segments: {error}"))?;
        Ok(Self {
            id: id.into(),
            output: path.to_path_buf(),
            segments,
            input_format: input_format.to_owned(),
            count: 0,
            preserve_segments: false,
        })
    }

    fn segment_name(&self) -> String {
        format!("segment-{:012}.{}", self.count, self.input_format)
    }

    fn remux(&self) -> Result<(), String> {
        let list = self.segments.join("concat.txt");
        let mut body = String::new();
        for index in 0..self.count {
            body.push_str(&format!(
                "file 'segment-{index:012}.{}'\n",
                self.input_format
            ));
        }
        fs::write(&list, body).map_err(|error| format!("write recording concat list: {error}"))?;
        let status = Command::new("ffmpeg")
            .current_dir(&self.segments)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                "concat.txt",
                "-c",
                "copy",
            ])
            .arg(&self.output)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("unable to start FFmpeg recording finalizer: {error}"))?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| format!("FFmpeg recording finalizer exited with status {status}"))
    }
}

impl BroadcastSink for FileRecordingSink {
    fn id(&self) -> &str {
        &self.id
    }

    fn push(&mut self, frame: &BroadcastFrame) -> Result<(), String> {
        let path = self.segments.join(self.segment_name());
        fs::write(path, &frame.encoded)
            .map_err(|error| format!("recording segment write failed: {error}"))?;
        self.count = self.count.saturating_add(1);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), String> {
        if self.count == 0 {
            return Err("recording contains no media frames".into());
        }
        match self.remux() {
            Ok(()) => {
                let _ = fs::remove_dir_all(&self.segments);
                Ok(())
            }
            Err(error) => {
                self.preserve_segments = true;
                Err(format!(
                    "{error}; recoverable segments preserved at {}",
                    self.segments.display()
                ))
            }
        }
    }
}

impl Drop for FileRecordingSink {
    fn drop(&mut self) {
        if !self.preserve_segments {
            let _ = fs::remove_dir_all(&self.segments);
        }
    }
}
