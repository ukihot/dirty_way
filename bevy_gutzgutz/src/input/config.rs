use std::collections::HashMap;

use super::map::{GutzAction, GutzInputMap};
use super::source::GutzInputSource;

/// TOML設定ファイルの生の形。1アクション名につき、割り当てたいDeviceの
/// 名前を複数並べる。
///
/// ```toml
/// [bindings]
/// rotate_left = ["KeyA"]
/// rotate_right = ["KeyD"]
/// charge = ["Space"]
/// restart = ["MouseLeft", "GamepadSouth"]
/// ```
#[derive(serde::Deserialize)]
struct GutzInputConfigToml {
    #[serde(default)]
    bindings: HashMap<String, Vec<String>>,
}

/// TOML文字列を読み込み、`resolve_action`でアクション名（文字列）をゲーム側の
/// Action型へ変換しながら`map`へバインディングを追加する。gutzgutzはゲーム
/// のAction型の文字列表現を一切知らないため、変換方法は呼び出し側に委ねる。
///
/// 解決できないアクション名・Device名は警告ログを出してスキップする
/// （設定ファイルの typo でゲーム全体が起動不能になるのを避ける）。
///
/// ```ignore
/// let mut map = GutzInputMap::<PlayerAction>::default();
/// gutz_input::load_into(&mut map, toml_str, |name| match name {
///     "rotate_left" => Some(PlayerAction::RotateLeft),
///     "rotate_right" => Some(PlayerAction::RotateRight),
///     "charge" => Some(PlayerAction::Charge),
///     "restart" => Some(PlayerAction::Restart),
///     _ => None,
/// })?;
/// ```
pub fn load_into<A: GutzAction>(
    map: &mut GutzInputMap<A>,
    toml_str: &str,
    resolve_action: impl Fn(&str) -> Option<A>,
) -> Result<(), toml::de::Error> {
    let config: GutzInputConfigToml = toml::from_str(toml_str)?;

    for (action_name, source_names) in config.bindings {
        let Some(action) = resolve_action(&action_name) else {
            bevy::log::warn!("gutzgutz input config: unknown action '{action_name}', skipping");
            continue;
        };
        for source_name in source_names {
            let Some(source) = GutzInputSource::parse(&source_name) else {
                bevy::log::warn!(
                    "gutzgutz input config: unknown input source '{source_name}' for action '{action_name}', skipping"
                );
                continue;
            };
            map.bind(action, source);
        }
    }

    Ok(())
}

/// ファイルパスから読み込む版。ファイルI/Oエラーは`std::io::Error`として、
/// パース/未知アクションのエラーは`ErrorKind::InvalidData`に詰めて返す。
pub fn load_into_from_file<A: GutzAction>(
    map: &mut GutzInputMap<A>,
    path: impl AsRef<std::path::Path>,
    resolve_action: impl Fn(&str) -> Option<A>,
) -> std::io::Result<()> {
    let contents = std::fs::read_to_string(path)?;
    load_into(map, &contents, resolve_action)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}
