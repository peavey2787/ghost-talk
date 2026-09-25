use crate::{BroadcastFrame, BroadcastSink, RtmpDestination};
use std::{
    io::Write,
    process::{Child, ChildStdin, Command, Stdio},
};

const RTMP_SAMPLE_RATE: &str = "48000";
const RTMP_CHANNELS: &str = "1";

/// RTMP/RTMPS adapter over the shared Ghost voice encoding.
///
/// Ghost voice units are decoded to PCM per unit and sent to one persistent
/// FFmpeg AAC/FLV publisher. This adapts the canonical Ghost encoded stream to
/// the codec required by conventional RTMP services without opening another
/// capture device or creating another application-owned encoder pipeline.
pub struct FfmpegRtmpSink {
    id: String,
    input_format: String,
    destination: RtmpDestination,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    reconnects: u8,
    max_reconnects: u8,
}

impl FfmpegRtmpSink {
    pub fn spawn(
        id: impl Into<String>,
        input_format: &str,
        destination: &RtmpDestination,
    ) -> Result<Self, String> {
        let mut sink = Self {
            id: id.into(),
            input_format: input_format.to_owned(),
            destination: destination.clone(),
            child: None,
            stdin: None,
            reconnects: 0,
            max_reconnects: 3,
        };
        sink.restart()?;
        Ok(sink)
    }

    fn restart(&mut self) -> Result<(), String> {
        self.close_child();
        let (child, stdin) = spawn_rtmp_encoder(&self.destination)?;
        self.child = Some(child);
        self.stdin = Some(stdin);
        Ok(())
    }

    fn write_pcm(&mut self, pcm: &[u8]) -> Result<(), String> {
        self.stdin
            .as_mut()
            .ok_or_else(|| "RTMP sink is closed".to_string())?
            .write_all(pcm)
            .map_err(|error| format!("RTMP write failed: {error}"))
    }

    fn close_child(&mut self) {
        drop(self.stdin.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.try_wait();
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl BroadcastSink for FfmpegRtmpSink {
    fn id(&self) -> &str {
        &self.id
    }

    fn push(&mut self, frame: &BroadcastFrame) -> Result<(), String> {
        let pcm = decode_unit(&self.input_format, &frame.encoded)?;
        if self.write_pcm(&pcm).is_ok() {
            self.reconnects = 0;
            return Ok(());
        }
        if self.reconnects >= self.max_reconnects {
            return Err("RTMP sink exceeded reconnect limit".into());
        }
        self.reconnects = self.reconnects.saturating_add(1);
        self.restart()?;
        self.write_pcm(&pcm)
    }

    fn finish(&mut self) -> Result<(), String> {
        drop(self.stdin.take());
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        let status = child
            .wait()
            .map_err(|error| format!("FFmpeg wait failed: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("FFmpeg RTMP exited with status {status}"))
        }
    }
}

fn decode_unit(input_format: &str, encoded: &[u8]) -> Result<Vec<u8>, String> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            input_format,
            "-i",
            "pipe:0",
            "-f",
            "s16le",
            "-ar",
            RTMP_SAMPLE_RATE,
            "-ac",
            RTMP_CHANNELS,
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("unable to start FFmpeg Ghost voice decoder: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "FFmpeg decoder stdin was unavailable".to_string())?
        .write_all(encoded)
        .map_err(|error| format!("FFmpeg decoder input failed: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("FFmpeg decoder wait failed: {error}"))?;
    output
        .status
        .success()
        .then_some(output.stdout)
        .ok_or_else(|| {
            format!(
                "FFmpeg Ghost voice decoder exited with status {}",
                output.status
            )
        })
}

fn spawn_rtmp_encoder(destination: &RtmpDestination) -> Result<(Child, ChildStdin), String> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-f",
            "s16le",
            "-ar",
            RTMP_SAMPLE_RATE,
            "-ac",
            RTMP_CHANNELS,
            "-i",
            "pipe:0",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-f",
            "flv",
        ])
        .arg(destination.publish_url())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("unable to start FFmpeg RTMP sink: {error}"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "FFmpeg RTMP stdin was unavailable".to_string())?;
    Ok((child, stdin))
}

impl Drop for FfmpegRtmpSink {
    fn drop(&mut self) {
        self.close_child();
    }
}
