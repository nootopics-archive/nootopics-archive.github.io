mod shared;

use chrono::DateTime;
use serde::Deserialize;
use shared::{ArchiveData, ChannelMeta, MinMsg, MinUser};
use std::borrow::Cow;
use std::collections::HashMap;
use std::{env, fs};
use std::io::Write;
use flate2::write::GzEncoder;
use flate2::Compression;

#[derive(Deserialize)]
struct RawExport {
    channel: RawChannel,
    messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawChannel {
    id: String,
    name: String,
    category: Option<String>,
}

#[derive(Deserialize)]
struct RawReference {
    #[serde(rename = "messageId")]
    message_id: Option<String>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: String,
    timestamp: String,
    #[serde(default)]
    content: String,
    #[serde(rename = "isPinned", default)]
    is_pinned: bool,
    author: RawAuthor,
    reference: Option<RawReference>,
}

#[derive(Deserialize)]
struct RawAuthor {
    id: String,
    name: String,
    nickname: Option<String>,
    color: Option<String>,
    #[serde(rename = "avatarUrl")]
    avatar_url: Option<String>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let target_dir = if args.len() > 1 { &args[1] } else { ".." };

    println!("📂 Scanning directory: {}", target_dir);

    let mut archive = ArchiveData {
        users: HashMap::new(),
        channels: Vec::new(),
        messages: HashMap::new(),
    };

    let paths = fs::read_dir(target_dir).expect("Failed to read directory");
    let mut files_parsed = 0;

    for path in paths {
        let path = path.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let content = fs::read_to_string(&path).unwrap();
            
            if let Ok(export) = serde_json::from_str::<RawExport>(&content) {
                println!("✅ Parsed: {}", file_name);
                files_parsed += 1;
                let mut min_msgs = Vec::new();
                let channel_id = export.channel.id.parse::<u64>().unwrap_or(0);

                for msg in export.messages {
                    let author_id = msg.author.id.parse::<u64>().unwrap_or(0);

                    archive.users.entry(author_id).or_insert_with(|| MinUser {
                        n: Cow::Owned(msg.author.nickname.clone().unwrap_or_else(|| msg.author.name.clone())),
                        u: Cow::Owned(msg.author.name.clone()),
                        c: msg.author.color.clone().map(Cow::Owned),
                        p: msg.author.avatar_url.clone().map(Cow::Owned),
                    });

                    let ts = DateTime::parse_from_rfc3339(&msg.timestamp)
                        .map(|dt| dt.timestamp_millis())
                        .unwrap_or(0);

                    let reply_id = msg.reference
                        .and_then(|r| r.message_id)
                        .and_then(|id| id.parse::<u64>().ok());

                    min_msgs.push(MinMsg {
                        i: msg.id.parse::<u64>().unwrap_or(0),
                        a: author_id,
                        c: Cow::Owned(msg.content),
                        t: ts,
                        p: msg.is_pinned,
                        r: reply_id,
                    });
                }

                min_msgs.sort_by_key(|m| m.t);

                archive.channels.push(ChannelMeta {
                    id: channel_id,
                    n: Cow::Owned(export.channel.name),
                    c: Cow::Owned(export.channel.category.unwrap_or_else(|| "Uncategorized".to_string())),
                });

                archive.messages.insert(channel_id, min_msgs);
            } else {
                println!("❌ Skipped (Invalid format): {}", file_name);
            }
        }
    }

    if files_parsed == 0 {
        println!("⚠️ No valid JSON files were found or parsed. Exiting.");
        return;
    }

    archive.channels.sort_by(|a, b| a.c.cmp(&b.c).then(a.n.cmp(&b.n)));

    println!("\n⚙️  Processing {} channels...", files_parsed);
    
    println!("📦 Serializing data to binary...");
    let bin_data = bincode::serialize(&archive).unwrap();
    
    println!("🗜️  Compressing data with GZIP (Best)...");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&bin_data).unwrap();
    let compressed_data = encoder.finish().unwrap();
    
    println!("💾 Saving archive.bin...");
    fs::write("archive.bin", compressed_data).unwrap();

    println!("🎉 Successfully generated archive.bin!");
}
