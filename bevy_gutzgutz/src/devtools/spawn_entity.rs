use bevy::prelude::*;

/// devtoolsの「その場で任意エンティティを生成する」機能のレジストリ。
/// ゲーム側が`register`で生成関数を登録し、gutzgutz側は名前の一覧表示と
/// 呼び出しだけを担当する。gutzgutzはゲームのプレファブ構造を一切知らない。
#[derive(Resource, Default)]
pub struct GutzSpawnRegistry {
    entries: Vec<(String, Box<dyn Fn(&mut Commands, Vec3) + Send + Sync>)>,
}

impl GutzSpawnRegistry {
    pub fn register(
        &mut self,
        name: impl Into<String>,
        spawn: impl Fn(&mut Commands, Vec3) + Send + Sync + 'static,
    ) {
        self.entries.push((name.into(), Box::new(spawn)));
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(name, _)| name.as_str())
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|(name, _)| name.as_str())
    }

    pub fn spawn_at(&self, index: usize, commands: &mut Commands, position: Vec3) {
        if let Some((_, spawn)) = self.entries.get(index) {
            spawn(commands, position);
        }
    }
}
