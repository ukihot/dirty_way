use bevy::prelude::*;
use std::collections::HashMap;

/// 任意のゲームがdevtoolsオーバーレイに行を追加できる汎用チャンネル
/// （doc/gutzgutz-requirements.md 5.1節）。ゲーム側は`set`で値を登録するだけで、
/// 表示方法（現在はegui、将来差し替わる可能性）はgutzgutz側が決める。
///
/// 挿入順を表示順として保つため、値本体（HashMap）とは別に挿入順の
/// キー一覧を保持する。
#[derive(Resource, Default)]
pub struct GutzDebugStats {
    values: HashMap<&'static str, String>,
    order: Vec<&'static str>,
}

impl GutzDebugStats {
    pub fn set(&mut self, key: &'static str, value: impl std::fmt::Display) {
        if !self.values.contains_key(key) {
            self.order.push(key);
        }
        self.values.insert(key, value.to_string());
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.order.iter().map(|key| (*key, self.values[key].as_str()))
    }
}
