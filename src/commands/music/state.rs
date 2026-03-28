use std::collections::HashSet;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoopMode {
    #[default]
    Off,
    Track,
    Queue,
}

/// Per-guild music settings and queue-loop snapshot (sources to replay).
#[derive(Debug, Clone)]
pub struct GuildMusicSettings {
    /// Playback volume multiplier for new tracks (0.0–2.0; Songbird uses ~0–1 typical).
    pub volume: f32,
    pub loop_mode: LoopMode,
    /// (query or URL, is_direct_url)
    pub queue_loop_snapshot: Vec<(String, bool)>,
}

impl Default for GuildMusicSettings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            loop_mode: LoopMode::Off,
            queue_loop_snapshot: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct MusicState {
    guilds: RwLock<std::collections::HashMap<u64, GuildMusicSettings>>,
    /// Guilds where we attached idle + queue-loop global handlers on the active Call.
    voice_hooks_installed: RwLock<HashSet<u64>>,
}

impl MusicState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn volume_for_guild(&self, guild_id: u64) -> f32 {
        self.guilds
            .read()
            .ok()
            .and_then(|g| g.get(&guild_id).map(|s| s.volume))
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1.0)
    }

    pub fn set_volume(&self, guild_id: u64, volume: f32) {
        let mut g = self.guilds.write().expect("music state lock");
        let e = g.entry(guild_id).or_default();
        e.volume = volume.clamp(0.0, 2.0);
    }

    pub fn set_loop_mode(&self, guild_id: u64, mode: LoopMode) {
        let mut g = self.guilds.write().expect("music state lock");
        let e = g.entry(guild_id).or_default();
        e.loop_mode = mode;
        if mode != LoopMode::Queue {
            e.queue_loop_snapshot.clear();
        }
    }

    pub fn loop_mode(&self, guild_id: u64) -> LoopMode {
        self.guilds
            .read()
            .ok()
            .and_then(|g| g.get(&guild_id).map(|s| s.loop_mode))
            .unwrap_or(LoopMode::Off)
    }

    pub fn set_queue_snapshot(&self, guild_id: u64, snapshot: Vec<(String, bool)>) {
        let mut g = self.guilds.write().expect("music state lock");
        let e = g.entry(guild_id).or_default();
        e.loop_mode = LoopMode::Queue;
        e.queue_loop_snapshot = snapshot;
    }

    pub fn queue_loop_snapshot(&self, guild_id: u64) -> Vec<(String, bool)> {
        self.guilds
            .read()
            .ok()
            .and_then(|g| g.get(&guild_id).map(|s| s.queue_loop_snapshot.clone()))
            .unwrap_or_default()
    }

    pub fn queue_loop_enabled(&self, guild_id: u64) -> bool {
        let Ok(g) = self.guilds.read() else {
            return false;
        };
        g.get(&guild_id)
            .map(|s| s.loop_mode == LoopMode::Queue && !s.queue_loop_snapshot.is_empty())
            .unwrap_or(false)
    }

    /// Returns true if this is the first time we should attach voice global events for this guild.
    pub fn try_install_voice_hooks(&self, guild_id: u64) -> bool {
        let mut s = self.voice_hooks_installed.write().expect("hooks lock");
        s.insert(guild_id)
    }

    pub fn clear_voice_hooks(&self, guild_id: u64) {
        let mut s = self.voice_hooks_installed.write().expect("hooks lock");
        s.remove(&guild_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_loop_enabled_requires_queue_mode_and_snapshot() {
        let m = MusicState::new();
        assert!(!m.queue_loop_enabled(1));
        m.set_queue_snapshot(1, vec![("https://example.com/watch?v=1".into(), true)]);
        assert!(m.queue_loop_enabled(1));
        m.set_loop_mode(1, LoopMode::Off);
        assert!(!m.queue_loop_enabled(1));
    }

    #[test]
    fn volume_defaults_to_one() {
        let m = MusicState::new();
        assert_eq!(m.volume_for_guild(99), 1.0);
        m.set_volume(99, 0.5);
        assert_eq!(m.volume_for_guild(99), 0.5);
    }
}
