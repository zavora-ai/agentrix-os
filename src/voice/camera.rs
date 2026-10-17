//! Camera channel for the Gemini Live session (M10-T5).
//!
//! When the user turns the camera on, the browser sends still frames over the voice websocket
//! (`{"type":"frame","mime":"image/jpeg","data":"<base64>"}`) and the server forwards them to the
//! model with `send_video_frame`. Suzy turns deliberate hand gestures into one `ui_gesture` tool
//! call, which the websocket relays to the client like every other tool call; the client maps it
//! to UI verbs (switch world, pause the agents, ask for the briefing) through the same routes a
//! click would use, so the permission gate and audit apply unchanged.
//!
//! Frames are forwarded and dropped. Nothing about them is stored, ledgered or logged
//! (ADR-004); the ledger only sees a content-free `ui_gesture` row from the client. This module
//! holds the pure parts: the gesture vocabulary, the instruction text and the frame gate.

use std::time::{Duration, Instant};

use serde_json::json;

/// Tool the model calls once per recognised gesture.
pub const TOOL_NAME: &str = "ui_gesture";

/// Deliberate hand gestures Suzy may report. What each one does is decided by the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    SwipeLeft,
    SwipeRight,
    OpenPalm,
    Wave,
    Pinch,
}

impl Gesture {
    pub const ALL: [Gesture; 5] = [Gesture::SwipeLeft, Gesture::SwipeRight, Gesture::OpenPalm, Gesture::Wave, Gesture::Pinch];

    pub fn as_str(&self) -> &'static str {
        match self {
            Gesture::SwipeLeft => "swipe_left",
            Gesture::SwipeRight => "swipe_right",
            Gesture::OpenPalm => "open_palm",
            Gesture::Wave => "wave",
            Gesture::Pinch => "pinch",
        }
    }

    pub fn parse(s: &str) -> Option<Gesture> {
        match s.trim().to_ascii_lowercase().as_str() {
            "swipe_left" => Some(Gesture::SwipeLeft),
            "swipe_right" => Some(Gesture::SwipeRight),
            "open_palm" => Some(Gesture::OpenPalm),
            "wave" => Some(Gesture::Wave),
            "pinch" => Some(Gesture::Pinch),
            _ => None,
        }
    }

    /// How the gesture looks in the frames.
    pub fn cue(&self) -> &'static str {
        match self {
            Gesture::SwipeLeft => "a hand sweeping from right to left",
            Gesture::SwipeRight => "a hand sweeping from left to right",
            Gesture::OpenPalm => "an open palm held still toward the camera for about a second",
            Gesture::Wave => "a wave",
            Gesture::Pinch => "a pinch — thumb and index finger brought together, like picking something up",
        }
    }

    /// What the client does with it.
    pub fn effect(&self) -> &'static str {
        match self {
            Gesture::SwipeLeft => "moves the view toward the Home world",
            Gesture::SwipeRight => "moves the view toward the Work world",
            Gesture::OpenPalm => "pauses the agents",
            Gesture::Wave => "asks for today's briefing",
            Gesture::Pinch => "closes the card in front (or the camera window when no card is open)",
        }
    }
}

/// JSON schema for the `ui_gesture` tool.
pub fn tool_parameters() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "gesture": {
                "type": "string",
                "enum": Gesture::ALL.iter().map(|g| g.as_str()).collect::<Vec<_>>(),
                "description": "The gesture you recognised in the camera frames"
            }
        },
        "required": ["gesture"]
    })
}

/// Appended to Suzy's instruction when the camera channel is on.
pub fn instruction() -> String {
    let mut s = String::from(
        "\n\nCamera: when the user turns the camera on you also receive still frames about once per second. \
         Use them only to recognise the deliberate hand gestures below and whether the user is present. \
         Do not describe what you see unless asked, and never mention people, objects, screens or text \
         in the frame. When you recognise a gesture, call ui_gesture exactly once for it and then stay \
         quiet unless the user speaks:",
    );
    for g in Gesture::ALL {
        s.push_str(&format!("\n- {} → ui_gesture {{\"gesture\": \"{}\"}}: {}.", g.cue(), g.as_str(), g.effect()));
    }
    // Measured 2026-10-17: with an empty frame and the user saying "here's my gesture", the model
    // reported a gesture every time (wave, open_palm, swipe_right) and, after a wave, also asked
    // for the briefing through submit_intent. Each of those acts on the user's behalf.
    s.push_str(
        "\nOnly call ui_gesture for a hand you can actually see making that gesture in the latest \
         frames. Never call it because the user says they are gesturing or asks you to guess: if no \
         hand making one of these gestures is visible, do not call it, and say you don't see a \
         gesture if they asked. If a gesture is unclear or accidental, do nothing. The interface \
         performs the gesture's action itself, so never call submit_intent or any other tool for \
         the same gesture.",
    );
    s
}

/// What `ui_gesture` returns to the model. Conclusive on purpose: a bare "relayed" with the
/// effect read like a request, and after a wave the model also called submit_intent for the
/// briefing the wave had already asked for.
pub fn tool_result(raw: &str) -> serde_json::Value {
    match Gesture::parse(raw) {
        Some(g) => json!({
            "status": "done",
            "gesture": g.as_str(),
            "effect": g.effect(),
            "next": "The interface has already done this. Do not call submit_intent or any other \
                     tool for this gesture. Say at most one short sentence, or nothing."
        }),
        None => json!({ "status": "error", "message": "unknown gesture; use one of the listed values" }),
    }
}

pub const ALLOWED_MIMES: [&str; 3] = ["image/jpeg", "image/webp", "image/png"];
/// Base64 payload cap: a 320-px JPEG at 60 % quality is ~15–40 KB.
pub const MAX_FRAME_BASE64_BYTES: usize = 256 * 1024;
/// At most 2.5 frames per second reach the model; the client sends about one.
pub const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(400);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameReject {
    CameraOff,
    Mime,
    Empty,
    TooLarge,
    TooFast,
}

impl FrameReject {
    pub fn as_str(&self) -> &'static str {
        match self {
            FrameReject::CameraOff => "camera_off",
            FrameReject::Mime => "unsupported_mime",
            FrameReject::Empty => "empty",
            FrameReject::TooLarge => "too_large",
            FrameReject::TooFast => "too_fast",
        }
    }
}

/// Per-connection admission control for frames. Counts are content-free and only for logs.
#[derive(Debug)]
pub struct FrameGate {
    enabled: bool,
    last: Option<Instant>,
    pub accepted: u64,
    pub rejected: u64,
}

impl FrameGate {
    pub fn new(enabled: bool) -> Self {
        Self { enabled, last: None, accepted: 0, rejected: 0 }
    }

    pub fn check(&mut self, mime: &str, data: &str, now: Instant) -> Result<(), FrameReject> {
        let verdict = self.verdict(mime, data, now);
        match verdict {
            Ok(()) => {
                self.last = Some(now);
                self.accepted += 1;
            }
            Err(_) => self.rejected += 1,
        }
        verdict
    }

    fn verdict(&self, mime: &str, data: &str, now: Instant) -> Result<(), FrameReject> {
        if !self.enabled {
            return Err(FrameReject::CameraOff);
        }
        if !ALLOWED_MIMES.contains(&mime) || data.starts_with("data:") {
            return Err(FrameReject::Mime);
        }
        if data.is_empty() {
            return Err(FrameReject::Empty);
        }
        if data.len() > MAX_FRAME_BASE64_BYTES {
            return Err(FrameReject::TooLarge);
        }
        if self.last.is_some_and(|last| now.duration_since(last) < MIN_FRAME_INTERVAL) {
            return Err(FrameReject::TooFast);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gestures_round_trip_and_reject_unknown() {
        for g in Gesture::ALL {
            assert_eq!(Gesture::parse(g.as_str()), Some(g));
            assert_eq!(Gesture::parse(&g.as_str().to_uppercase()), Some(g));
        }
        assert_eq!(Gesture::parse("thumbs_up"), None);
        assert_eq!(Gesture::parse(""), None);
    }

    #[test]
    fn instruction_and_schema_name_every_gesture() {
        let text = instruction();
        let schema = tool_parameters();
        let allowed = schema["properties"]["gesture"]["enum"].as_array().unwrap();
        for g in Gesture::ALL {
            assert!(text.contains(g.as_str()), "{}", g.as_str());
            assert!(text.contains(g.cue()));
            assert!(allowed.iter().any(|v| v == g.as_str()));
        }
        assert!(text.contains("never mention people"), "privacy rule stays in the prompt");
        assert!(text.contains("you can actually see"), "no gesture the model cannot see");
        assert!(text.contains("never call submit_intent"), "a gesture is not also an intent");
        assert_eq!(schema["required"][0], "gesture");
        assert_eq!(TOOL_NAME, "ui_gesture");
    }

    #[test]
    fn tool_result_is_conclusive_and_rejects_unknown_gestures() {
        let done = tool_result("Wave");
        assert_eq!(done["status"], "done");
        assert_eq!(done["gesture"], "wave");
        assert!(done["next"].as_str().unwrap().contains("Do not call submit_intent"));
        assert_eq!(tool_result("thumbs_up")["status"], "error");
    }

    #[test]
    fn frame_gate_enforces_flag_mime_size_and_rate() {
        let t0 = Instant::now();
        let mut off = FrameGate::new(false);
        assert_eq!(off.check("image/jpeg", "abc", t0), Err(FrameReject::CameraOff));

        let mut gate = FrameGate::new(true);
        assert_eq!(gate.check("text/plain", "abc", t0), Err(FrameReject::Mime));
        assert_eq!(gate.check("image/jpeg", "data:image/jpeg;base64,abc", t0), Err(FrameReject::Mime));
        assert_eq!(gate.check("image/jpeg", "", t0), Err(FrameReject::Empty));
        let big = "A".repeat(MAX_FRAME_BASE64_BYTES + 1);
        assert_eq!(gate.check("image/jpeg", &big, t0), Err(FrameReject::TooLarge));
        assert_eq!(gate.check("image/jpeg", "abc", t0), Ok(()));
        assert_eq!(gate.check("image/webp", "abc", t0 + Duration::from_millis(100)), Err(FrameReject::TooFast));
        assert_eq!(gate.check("image/webp", "abc", t0 + MIN_FRAME_INTERVAL), Ok(()));
        assert_eq!((gate.accepted, gate.rejected), (2, 5));
    }
}
