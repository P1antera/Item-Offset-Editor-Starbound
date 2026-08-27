use serde_json::{json, Map, Value};
use std::{fs, path::{Path, PathBuf}};

fn err(message: impl Into<String>) -> String { message.into() }
// Starbound 的 JSON 文件允许 // 与 /* ... */ 注释，也接受字符串内的原始换行；serde_json 不允许。
// 字符串中的网址、转义引号和 "//" 保持原样，原始换行会规范化成 JSON 的 \n 转义。
fn strip_json_comments(raw: &str) -> String {
  let chars: Vec<char>=raw.chars().collect(); let mut output=String::with_capacity(raw.len()); let mut index=0; let mut in_string=false; let mut escaped=false;
  while index<chars.len() {
    let current=chars[index];
    if in_string {
      if current=='\r' || current=='\n' { output.push('\\'); output.push('n'); if current=='\r' && chars.get(index+1)==Some(&'\n') { index+=1; } index+=1; continue; }
      output.push(current);
      if escaped { escaped=false; } else if current=='\\' { escaped=true; } else if current=='"' { in_string=false; }
      index+=1; continue;
    }
    if current=='"' { in_string=true; output.push(current); index+=1; continue; }
    if current=='/' && chars.get(index+1)==Some(&'/') { index+=2; while index<chars.len() && chars[index]!='\n' && chars[index]!='\r' { index+=1; } continue; }
    if current=='/' && chars.get(index+1)==Some(&'*') { index+=2; while index+1<chars.len() && !(chars[index]=='*' && chars[index+1]=='/') { if chars[index]=='\n' || chars[index]=='\r' { output.push(chars[index]); } index+=1; } if index+1<chars.len() { index+=2; } continue; }
    output.push(current); index+=1;
  }
  output
}
fn parse_jsonc(raw: &str) -> Result<Value, serde_json::Error> { serde_json::from_str(&strip_json_comments(raw)) }
fn root_dir() -> PathBuf { std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) }
fn game_root() -> PathBuf {
  if let Some(root) = std::env::var_os("STARBOUND_ROOT").map(PathBuf::from) { return root; }
  let executable=std::env::current_exe().ok().and_then(|path|path.parent().map(Path::to_path_buf));
  let current=root_dir();
  for start in executable.iter().chain(std::iter::once(&current)) {
    for candidate in start.ancestors() { if candidate.join("assets/packed").is_dir() { return candidate.to_path_buf(); } }
  }
  executable.unwrap_or_else(root_dir)
}

#[tauri::command]
fn default_root() -> String { game_root().to_string_lossy().to_string() }

#[tauri::command]
fn choose_starbound_root() -> Option<String> { rfd::FileDialog::new().set_title("选择 Starbound 根目录").pick_folder().map(|path|path.to_string_lossy().to_string()) }

#[tauri::command]
fn choose_activeitem() -> Option<String> { rfd::FileDialog::new().set_title("选择 ActiveItem 文件").add_filter("Starbound ActiveItem", &["activeitem"]).pick_file().map(|path|path.to_string_lossy().to_string()) }

#[tauri::command]
fn load_activeitem(path: String) -> Result<Value, String> {
  let raw = fs::read_to_string(&path).map_err(|e| err(format!("无法读取 ActiveItem：{e}")))?;
  parse_jsonc(&raw).map_err(|e| err(format!("ActiveItem JSON 无效：{e}")))
}

fn scan_stances(value: &Value, prefix: &str, found: &mut Vec<String>) {
  match value {
    Value::Object(map) => {
      if map.contains_key("weaponOffset") || map.contains_key("armRotation") || map.contains_key("weaponRotation") { found.push(prefix.trim_end_matches('.').to_string()); }
      for (key, child) in map { scan_stances(child, &format!("{prefix}{key}."), found); }
    }
    Value::Array(items) => for (index, child) in items.iter().enumerate() { scan_stances(child, &format!("{prefix}{index}."), found); },
    _ => {}
  }
}

#[tauri::command]
fn find_stances(value: Value) -> Vec<String> { let mut found=Vec::new(); scan_stances(&value,"",&mut found); found.sort(); found.dedup(); found }

fn data_url(path: &Path) -> Option<String> { let bytes=fs::read(path).ok()?; Some(format!("data:image/png;base64,{}", base64_encode(&bytes))) }
fn base64_encode(data: &[u8]) -> String { const T:&[u8;64]=b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"; let mut out=String::new(); for c in data.chunks(3) { let n=((c[0]as u32)<<16)|((c.get(1).copied().unwrap_or(0)as u32)<<8)|c.get(2).copied().unwrap_or(0)as u32; out.push(T[(n>>18)as usize]as char);out.push(T[((n>>12)&63)as usize]as char);out.push(if c.len()>1{T[((n>>6)&63)as usize]as char}else{'='});out.push(if c.len()>2{T[(n&63)as usize]as char}else{'='}); }out }
fn first_existing(paths: Vec<PathBuf>) -> Option<PathBuf> { paths.into_iter().find(|p|p.is_file()) }
fn read_frames(path: &Path) -> Option<Value> { fs::read_to_string(path).ok().and_then(|raw| parse_jsonc(&raw).ok()) }
fn sprite_layer(image: &Path, frames: Option<&Path>) -> Option<Value> { data_url(image).map(|image| json!({"image": image, "frames": frames.and_then(read_frames)})) }
// Mod weapons often use <image-name>.frames instead of default.frames. The image-specific table
// has priority because it describes the exact spritesheet selected by animationParts.
fn frames_for_image(image: &Path, default_frames: &Path) -> Option<PathBuf> {
  let sibling=image.with_extension("frames");
  if sibling.is_file() { Some(sibling) } else if default_frames.is_file() { Some(default_frames.to_path_buf()) } else { None }
}
fn image_path(root: &Path, active: &Path, name: &str) -> PathBuf {
  let clean=name.split('?').next().unwrap_or(name);
  if !clean.starts_with('/') { return active.parent().unwrap_or(Path::new(".")).join(clean); }
  // Mod 的虚拟绝对路径首先在该 Mod 根目录中解析，随后才回退到 packed 资源。
  let mut ancestor=active.parent();
  while let Some(directory)=ancestor {
    if directory.join("items").is_dir() {
      let candidate=directory.join(clean.trim_start_matches('/'));
      if candidate.is_file() { return candidate; }
    }
    ancestor=directory.parent();
  }
  root.join("assets/packed").join(clean.trim_start_matches('/'))
}
fn base_offset(value: &Value) -> Value {
  value.get("baseOffset").cloned().unwrap_or_else(||json!([0.0, 0.0]))
}
#[tauri::command]
fn preview_assets(root: String, activeitem_path: String, species: String) -> Map<String, Value> {
  let active=PathBuf::from(&activeitem_path); let root=PathBuf::from(root); let humanoid=root.join("assets/packed/humanoid").join(&species); let generic=root.join("assets/packed/humanoid");
  let mut result=Map::new(); result.insert("humanoidConfig".into(),read_frames(&root.join("assets/packed/humanoid.config")).unwrap_or_else(||json!({}))); for (key,image_name,frames_name) in [("body","malebody.png","malebody.frames"),("frontArm","frontarm.png","frontarm.frames"),("backArm","backarm.png","backarm.frames"),("head","malehead.png","malehead.frames")] { if let Some(layer)=sprite_layer(&humanoid.join(image_name),Some(&generic.join(frames_name))){result.insert(key.into(),layer);} }
  if let Some(layer)=sprite_layer(&humanoid.join("hair/male1.png"),Some(&humanoid.join("hair/default.frames"))){result.insert("hair".into(),layer);}
  if let Some(layer)=sprite_layer(&humanoid.join("emote.png"),Some(&generic.join("emote.frames"))){result.insert("face".into(),layer);}
  let active_json=fs::read_to_string(&active).ok().and_then(|raw|parse_jsonc(&raw).ok()).unwrap_or(Value::Null);
  // 固定武器：animationParts 中的相对 PNG 路径。优先 gun / weapon / blade，随后取首个有效部件。
  let fixed_names=["gun","weapon","bow","boomerang","blade","middle","body","handle"];
  let configured=active_json.get("animationParts").and_then(Value::as_object);
  let direct_part=fixed_names.iter().find_map(|key|configured.and_then(|p|p.get(*key)).and_then(Value::as_str).filter(|v|!v.is_empty()).map(|name|((*key).to_string(),name.to_string())) )
    .or_else(||configured.and_then(|p|p.iter().find_map(|(key,v)|v.as_str().filter(|s|!s.is_empty()).map(|name|(key.clone(),name.to_string())))));
  let item=direct_part.as_ref().map(|(_,name)|image_path(&root,&active,name)).filter(|path|path.is_file()).or_else(||first_existing(vec![active.parent().unwrap_or(Path::new(".")).join("body.png"),active.parent().unwrap_or(Path::new(".")).join("gun.png")]));
  if let Some(path)=item {
    let default_frames=active.parent().unwrap_or(Path::new(".")).join("default.frames");
    let item_frames=frames_for_image(&path,&default_frames);
    if let Some(layer)=sprite_layer(&path,item_frames.as_deref()){result.insert("item".into(),layer);}
    let part_name=direct_part.as_ref().map(|(key,_)|key.as_str()).unwrap_or("gun");
    let animation=active_json.get("animation").and_then(Value::as_str).and_then(|name|read_frames(&image_path(&root,&active,name))).unwrap_or(Value::Null);
    let animation_offset=animation.pointer(&format!("/animatedParts/parts/{part_name}/properties/offset")).cloned();
    let custom_offset=active_json.pointer(&format!("/animationCustom/animatedParts/parts/{part_name}/properties/offset")).cloned();
    let built_offset=if part_name=="middle" && active_json.get("builder").and_then(Value::as_str).map(|b|b.ends_with("buildunrandweapon.lua")).unwrap_or(false) { Some(base_offset(&active_json)) } else { None };
    let (offset,source)=if let Some(value)=built_offset { (value,"baseOffset") } else if let Some(value)=custom_offset { (value,"animationCustom") } else if let Some(value)=animation_offset { (value,"animation") } else { (json!([0.0,0.0]),"animationCustom") };
    result.insert("itemPartOffset".into(),offset);
    result.insert("itemPartName".into(),Value::String(part_name.to_string()));
    result.insert("itemPartOffsetSource".into(),Value::String(source.to_string()));
  }

  // 随机武器：复刻 buildweapon.lua 的 gunParts 顺序，将 butt / middle / barrel 等变体拼接成一支枪。
  // 变体编号由前端显式传入，便于和游戏中的随机结果逐一对照。
  if result.get("item").is_none() {
    if let Some(config)=active_json.get("builderConfig").and_then(Value::as_array).and_then(|v|v.first()) {
      if let (Some(parts),Some(order))=(config.get("animationParts").and_then(Value::as_object),config.get("gunParts").and_then(Value::as_array)) {
        let mut layers=Vec::new();
        for key in order.iter().filter_map(Value::as_str) {
          let Some(definition)=parts.get(key) else { continue };
          let Some(path_template)=definition.get("path").and_then(Value::as_str) else { continue };
          let chosen=1_u32;
          let path=image_path(&root,&active,&path_template.replace("<variant>",&chosen.to_string()));
          if let Some(layer)=sprite_layer(&path,None) { layers.push(layer); }
        }
        if !layers.is_empty() { let offset=base_offset(&active_json); result.insert("generatedWeapon".into(),json!({"parts":layers,"baseOffset":offset})); result.insert("itemPartOffset".into(),base_offset(&active_json)); result.insert("itemPartName".into(),Value::String("middle".into())); result.insert("itemPartOffsetSource".into(),Value::String("baseOffset".into())); }
      }
    }
  }
  result
}

fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![default_root,choose_starbound_root,choose_activeitem,load_activeitem,find_stances,preview_assets]).run(tauri::generate_context!()).expect("error while running application"); }

#[cfg(test)]
mod tests {
  use super::parse_jsonc;
  use std::{fs, path::Path};

  #[test]
  fn accepts_starbound_json_comments_without_changing_strings() {
    let value=parse_jsonc(r#"{
      // disabled weapon mode
      "url": "https://example.invalid//keep",
      "value": 1, /* temporary note */
      "quote": "a \"//\" string"
    }"#).expect("JSONC should parse");
    assert_eq!(value["url"], "https://example.invalid//keep");
    assert_eq!(value["quote"], "a \"//\" string");
  }

  #[test]
  fn normalizes_raw_newlines_inside_starbound_strings() {
    let value=parse_jsonc("{\"description\": \"first line\r\nsecond line\"}").expect("raw string newline should parse");
    assert_eq!(value["description"], "first line\nsecond line");
  }

  #[test]
  fn accepts_the_reported_mod_activeitems() {
    let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for path in [
      "mods/at_ext/items/active/weapons/at_ext_rf5/at_ext_rf5_2.activeitem",
      "OpenStarbound/mods/at_ext/items/active/weapons/at_ext_sg1/at_ext_sg1.activeitem",
      "OpenStarbound/mods/at_ext/items/active/weapons/at_ext_rl2/at_ext_rl2.activeitem",
      "OpenStarbound/mods/at_ext/items/active/weapons/at_ext_snp/at_ext_snp.activeitem",
    ] {
      let raw=fs::read_to_string(root.join(path)).expect("reported Mod file should exist");
      parse_jsonc(&raw).unwrap_or_else(|error| panic!("{path} should parse: {error}"));
    }
  }

  #[test]
  fn selects_rf6s_image_specific_frame_table() {
    let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let image=root.join("OpenStarbound/mods/at_ext/items/active/weapons/at_ext_rf6/at_ext_rf6.png");
    let frames=super::frames_for_image(&image,&image.with_file_name("default.frames"));
    assert_eq!(frames,Some(image.with_extension("frames")));
  }
}
