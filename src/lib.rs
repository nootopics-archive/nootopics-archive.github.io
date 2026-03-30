mod shared;

use chrono::{Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Utc};
use shared::ArchiveData;
use std::collections::HashMap;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

lazy_static::lazy_static! {
    static ref ARCHIVE: Mutex<Option<ArchiveData<'static>>> = Mutex::new(None);
    static ref SEARCH_RESULTS: Mutex<Vec<(u64, usize)>> = Mutex::new(Vec::new());
    static ref STATS: Mutex<Option<StatsCache>> = Mutex::new(None);
}

struct DayInfo {
    count: usize,
    first_msg_t: i64,
    first_msg_c: u64,
    first_msg_idx: usize,
}

struct StatsCache {
    leaderboard: Vec<(u64, usize)>,
    daily_counts: HashMap<NaiveDate, DayInfo>,
    max_day: usize,
}

#[wasm_bindgen]
pub fn init_engine(raw_bin_data: &[u8]) {
    let owned_data = raw_bin_data.to_vec();
    let leaked_data: &'static [u8] = owned_data.leak();
        
    let data: ArchiveData<'static> = bincode::deserialize(leaked_data)
        .expect("Failed to deserialize");
        
    *ARCHIVE.lock().unwrap() = Some(data);
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&#39;")
}

fn format_number(n: usize) -> String {
    let n_str = n.to_string();
    let mut s = String::new();
    let chars: Vec<char> = n_str.chars().collect();
    for (i, c) in chars.iter().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            s.push(',');
        }
        s.push(*c);
    }
    s.chars().rev().collect()
}

// ==========================================
// STATS & LEADERBOARD LOGIC
// ==========================================

#[wasm_bindgen]
pub fn compute_stats(tz_offset_mins: i32) {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();

    let mut user_counts: HashMap<u64, usize> = HashMap::new();
    let mut daily_counts: HashMap<NaiveDate, DayInfo> = HashMap::new();
    let mut max_day = 0;

    // JS getTimezoneOffset() returns minutes *ahead* of UTC. So UTC-5 (EST) returns 300.
    // FixedOffset::west_opt(300 * 60) correctly creates a -05:00 offset.
    let tz = FixedOffset::west_opt(tz_offset_mins * 60).unwrap_or_else(|| FixedOffset::west_opt(0).unwrap());

    for (&c_id, msgs) in &data.messages {
        for (idx, msg) in msgs.iter().enumerate() {
            // Tally Leaderboard
            *user_counts.entry(msg.a).or_insert(0) += 1;

            // Tally Heatmap
            let local_time = Utc.timestamp_millis_opt(msg.t).unwrap().with_timezone(&tz);
            let date = local_time.date_naive();

            let day_info = daily_counts.entry(date).or_insert(DayInfo {
                count: 0,
                first_msg_t: msg.t,
                first_msg_c: c_id,
                first_msg_idx: idx,
            });

            day_info.count += 1;
            
            // Keep track of the earliest message on this day for the jump link
            if msg.t < day_info.first_msg_t {
                day_info.first_msg_t = msg.t;
                day_info.first_msg_c = c_id;
                day_info.first_msg_idx = idx;
            }
        }
    }

    for info in daily_counts.values() {
        if info.count > max_day {
            max_day = info.count;
        }
    }

    let mut leaderboard: Vec<_> = user_counts.into_iter().filter(|(uid, _)| {
        // Filter out the fallback ID 0 just in case
        if *uid == 0 {
            return false;
        }
        
        // Filter out any user named "Deleted User"
        if let Some(user) = data.users.get(uid) {
            if user.u == "Deleted User" || user.n == "Deleted User" {
                return false;
            }
        }
        true
    }).collect();

    leaderboard.sort_by(|a, b| b.1.cmp(&a.1));

    *STATS.lock().unwrap() = Some(StatsCache {
        leaderboard,
        daily_counts,
        max_day,
    });
}

#[wasm_bindgen]
pub fn get_leaderboard_html(start_idx: usize, end_idx: usize) -> String {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();
    let stats_lock = STATS.lock().unwrap();
    let stats = stats_lock.as_ref().unwrap();

    let end = std::cmp::min(end_idx + 1, stats.leaderboard.len());
    if start_idx >= end {
        return String::new();
    }

    let mut html = String::new();
    for (i, (uid, count)) in stats.leaderboard[start_idx..end].iter().enumerate() {
        let rank = start_idx + i + 1;
        if let Some(user) = data.users.get(uid) {
            let safe_name = escape_html(&user.n);
            let avatar_html = if let Some(pfp) = &user.p {
                format!("<div class=\"lb-avatar avatar-wrapper\"><img src=\"__DEFAULT_PFP__\" class=\"avatar-layer\"><img src=\"{}\" loading=\"lazy\" class=\"avatar-layer\" style=\"opacity: 0;\" onload=\"this.style.opacity=1; this.previousElementSibling.style.display='none';\"></div>", pfp)
            } else {
                "<img src=\"__DEFAULT_PFP__\" class=\"lb-avatar\">".to_string()
            };

            html.push_str(&format!(
                "<div class=\"lb-item\">
                    <div class=\"lb-rank\">#{}</div>
                    {}
                    <div class=\"lb-name\" title=\"{}\">{}</div>
                    <div class=\"lb-count\">{}</div>
                </div>",
                rank, avatar_html, escape_html(&user.u), safe_name, format_number(*count)
            ));
        }
    }
    html
}

#[wasm_bindgen]
pub fn get_heatmap_years() -> String {
    let stats_lock = STATS.lock().unwrap();
    let stats = stats_lock.as_ref().unwrap();

    let mut years: Vec<i32> = stats.daily_counts.keys().map(|d| d.year()).collect();
    years.sort_unstable();
    years.dedup();

    let mut json = String::from("[");
    for (i, y) in years.iter().enumerate() {
        if i > 0 { json.push(','); }
        json.push_str(&y.to_string());
    }
    json.push(']');
    json
}

// Helper function to calculate a continuous Viridis color scale
fn get_continuous_color(count: usize, max: usize) -> String {
    if count == 0 {
        return "rgba(255,255,255,0.05)".to_string();
    }
    
    // Linear scale: direct percentage of the max day
    let mut t = (count as f64) / (max as f64);
    t = t.clamp(0.0, 1.0);

    // Viridis color stops
    let stops = [
        (68.0, 1.0, 84.0),     // 0.00: Dark Purple
        (59.0, 82.0, 139.0),   // 0.25: Blue
        (33.0, 145.0, 140.0),  // 0.50: Teal
        (94.0, 201.0, 98.0),   // 0.75: Green
        (253.0, 231.0, 37.0),  // 1.00: Yellow
    ];

    let idx = (t * 4.0).floor() as usize;
    if idx >= 4 {
        return format!("rgb({}, {}, {})", stops[4].0 as u8, stops[4].1 as u8, stops[4].2 as u8);
    }

    // Linear interpolation between the two closest stops
    let local_t = (t * 4.0) - (idx as f64);
    let c1 = stops[idx];
    let c2 = stops[idx + 1];

    let r = (c1.0 + (c2.0 - c1.0) * local_t) as u8;
    let g = (c1.1 + (c2.1 - c1.1) * local_t) as u8;
    let b = (c1.2 + (c2.2 - c1.2) * local_t) as u8;

    format!("rgb({}, {}, {})", r, g, b)
}

#[wasm_bindgen]
pub fn get_heatmap_html(year: i32) -> String {
    let stats_lock = STATS.lock().unwrap();
    let stats = stats_lock.as_ref().unwrap();

    let mut html = String::new();
    let start_date = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let end_date = NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap();

    // CSS Grid fills top-to-bottom (Sunday to Saturday).
    // If Jan 1st is a Tuesday (2 days from Sunday), we insert 2 empty hidden blocks first.
    let empty_days = start_date.weekday().num_days_from_sunday();
    for _ in 0..empty_days {
        html.push_str("<div class=\"hm-cell hidden-cell\"></div>");
    }

    let mut curr = start_date;

    while curr < end_date {
        let date_str = curr.format("%b %-d, %Y").to_string();

        if let Some(info) = stats.daily_counts.get(&curr) {
            let color = get_continuous_color(info.count, stats.max_day);
            let title = format!("{}: {} messages", date_str, format_number(info.count));
            
            html.push_str(&format!(
                "<div class=\"hm-cell\" style=\"background-color: {};\" title=\"{}\" onclick=\"jumpToMessage('{}', {}); closeStats();\"></div>",
                color, title, info.first_msg_c, info.first_msg_idx
            ));
        } else {
            let title = format!("{}: 0 messages", date_str);
            html.push_str(&format!("<div class=\"hm-cell empty-cell\" title=\"{}\"></div>", title));
        }
        curr += Duration::days(1);
    }
    html
}

// ==========================================
// EXISTING UI & SEARCH LOGIC
// ==========================================

#[wasm_bindgen]
pub fn get_sidebar_html() -> String {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();
    let mut html = String::new();
    let mut current_cat = String::new();

    for ch in &data.channels {
        if ch.c.as_ref() != current_cat {
            html.push_str(&format!("<div class=\"category\">{}</div>", escape_html(&ch.c)));
            current_cat = ch.c.to_string();
        }
        html.push_str(&format!(
            "<div class=\"channel-btn\" id=\"ch-{}\" onclick=\"selectChannel('{}')\">{}</div>",
            ch.id, ch.id, escape_html(&ch.n)
        ));
    }
    html
}

#[wasm_bindgen]
pub fn get_channel_name(channel_id: &str) -> String {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();
    let c_id = channel_id.parse::<u64>().unwrap_or(0);
    data.channels.iter()
        .find(|c| c.id == c_id)
        .map(|c| format!("# {}", c.n))
        .unwrap_or_else(|| "Unknown".to_string())
}

#[wasm_bindgen]
pub fn get_message_count(channel_id: &str, only_pins: bool) -> usize {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();
    let c_id = channel_id.parse::<u64>().unwrap_or(0);
    if let Some(msgs) = data.messages.get(&c_id) {
        if only_pins {
            msgs.iter().filter(|m| m.p).count()
        } else {
            msgs.len()
        }
    } else {
        0
    }
}

#[wasm_bindgen]
pub fn get_msg_global_idx(channel_id: &str, msg_id: u64) -> i32 {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();
    let c_id = channel_id.parse::<u64>().unwrap_or(0);
    if let Some(msgs) = data.messages.get(&c_id) {
        for (idx, m) in msgs.iter().enumerate() {
            if m.i == msg_id {
                return idx as i32;
            }
        }
    }
    -1
}

#[wasm_bindgen]
pub fn get_messages_html(channel_id: &str, start_idx: usize, end_idx: usize, only_pins: bool) -> String {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();
    let c_id = channel_id.parse::<u64>().unwrap_or(0);
    
    let msgs = match data.messages.get(&c_id) {
        Some(m) => m,
        None => return String::new(),
    };

    let mut html = String::new();
    let mut last_author: u64 = 0;
    let mut last_time = 0;

    let iter: Box<dyn Iterator<Item = (usize, &crate::shared::MinMsg)>> = if only_pins {
        Box::new(msgs.iter().enumerate().filter(|(_, m)| m.p).rev())
    } else {
        Box::new(msgs.iter().enumerate().skip(start_idx).take(end_idx - start_idx + 1))
    };

    for (global_idx, msg) in iter {
        let user = data.users.get(&msg.a).unwrap();
        let safe_content = if msg.c.trim().is_empty() {
            "<span style=\"color: var(--text-muted); font-style: italic;\">[attachment]</span>".to_string()
        } else {
            escape_html(&msg.c)
        };
        let safe_name = escape_html(&user.n);
        let safe_username = escape_html(&user.u);
        let color = user.c.as_deref().unwrap_or("#ffffff");

        let mut is_grouped = !only_pins && msg.a == last_author && (msg.t - last_time) < 300000;
        let mut reply_html = String::new();
        let mut has_reply_class = "";

        if let Some(reply_id) = msg.r {
            is_grouped = false; 
            has_reply_class = "has-reply"; 
            
            if let Ok(orig_idx) = msgs.binary_search_by_key(&reply_id, |m| m.i) {
                let orig_msg = &msgs[orig_idx];
                let orig_user = data.users.get(&orig_msg.a).unwrap();
                let orig_color = orig_user.c.as_deref().unwrap_or("#ffffff");
                let orig_name = escape_html(&orig_user.n);
                let orig_username = escape_html(&orig_user.u);
                
                let mut orig_content = orig_msg.c.replace('\n', " ");
                if orig_content.trim().is_empty() {
                    orig_content = "Attachment".to_string();
                } else if orig_content.chars().count() > 80 {
                    orig_content = orig_content.chars().take(80).collect::<String>() + "...";
                }
                let safe_orig_content = escape_html(&orig_content);

                let orig_avatar_html = if let Some(pfp) = &orig_user.p {
                    format!("<div class=\"reply-avatar avatar-wrapper\"><img src=\"__DEFAULT_PFP__\" class=\"avatar-layer\"><img src=\"{}\" loading=\"lazy\" class=\"avatar-layer\" style=\"opacity: 0;\" onload=\"this.style.opacity=1; this.previousElementSibling.style.display='none';\"></div>", pfp)
                } else {
                    "<img src=\"__DEFAULT_PFP__\" class=\"reply-avatar\">".to_string()
                };

                reply_html = format!(
                    "<div class=\"replied-message\" onclick=\"jumpToMessageById('{}', '{}')\">
                        <div class=\"reply-spine\"></div>
                        {}
                        <span class=\"reply-author\" style=\"color: {}\" title=\"{}\">{}</span>
                        <span class=\"reply-text\">{}</span>
                    </div>",
                    channel_id, reply_id, orig_avatar_html, orig_color, orig_username, orig_name, safe_orig_content
                );
            } else {
                reply_html = format!(
                    "<div class=\"replied-message\">
                        <div class=\"reply-spine\"></div>
                        <span class=\"reply-text\" style=\"font-style: italic;\">Original message was deleted.</span>
                    </div>"
                );
            }
        }

        last_author = msg.a;
        last_time = msg.t;

        let grouped_class = if is_grouped { "grouped" } else { "" };
        let classes = format!("{} {}", grouped_class, has_reply_class);

        let avatar_html = if is_grouped {
            format!("<span class=\"msg-timestamp-hover ts-time\" data-ts=\"{}\"></span>", msg.t)
        } else if let Some(pfp) = &user.p {
            format!("<div class=\"avatar avatar-wrapper\"><img src=\"__DEFAULT_PFP__\" class=\"avatar-layer\"><img src=\"{}\" loading=\"lazy\" class=\"avatar-layer\" style=\"opacity: 0;\" onload=\"this.style.opacity=1; this.previousElementSibling.style.display='none';\"></div>", pfp)
        } else {
            "<img src=\"__DEFAULT_PFP__\" class=\"avatar\">".to_string()
        };

        let header_html = if is_grouped {
            format!("<div class=\"msg-actions grouped-actions\" onclick=\"copyLink('{}', '{}')\">🔗 Copy Link</div>", channel_id, msg.i)
        } else {
            format!("<div class=\"msg-header\"><span class=\"name\" style=\"color: {}\" title=\"{}\">{}</span><span class=\"time ts-full\" data-ts=\"{}\"></span><span class=\"msg-actions\" onclick=\"copyLink('{}', '{}')\">🔗 Copy Link</span></div>", color, safe_username, safe_name, msg.t, channel_id, msg.i)
        };

        html.push_str(&format!(
            "<div class=\"msg {}\" id=\"msg-{}\">
                {}
                <div class=\"msg-content\">
                    {}
                    {}
                    <div class=\"text\">{}</div>
                </div>
            </div>",
            classes, global_idx, avatar_html, reply_html, header_html, safe_content
        ));
    }
    html
}

#[wasm_bindgen]
pub fn execute_search(query: &str, global: bool, current_channel: &str) -> usize {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();
    
    let mut query_lower = query.to_lowercase();
    let mut results = Vec::new();

    let channels_to_search: Vec<u64> = if global {
        data.messages.keys().copied().collect()
    } else {
        let c_id = current_channel.parse::<u64>().unwrap_or(0);
        if c_id != 0 { vec![c_id] } else { vec![] }
    };

    let mut target_author = None;
    if let Some(idx) = query_lower.find("from:") {
        let after_from = &query_lower[idx + 5..];
        let end_idx = after_from.find(' ').unwrap_or(after_from.len());
        target_author = Some(after_from[..end_idx].to_string());
        
        let remove_str = format!("from:{}", target_author.as_ref().unwrap());
        query_lower = query_lower.replace(&remove_str, "").trim().to_string();
    }

    for c_id in channels_to_search {
        if let Some(msgs) = data.messages.get(&c_id) {
            for (global_idx, msg) in msgs.iter().enumerate() {
                let user = data.users.get(&msg.a).unwrap();
                
                if let Some(target) = &target_author {
                    if !user.u.to_lowercase().contains(target) && !msg.a.to_string().contains(target) {
                        continue;
                    }
                }

                if !query_lower.is_empty() && !msg.c.to_lowercase().contains(&query_lower) {
                    continue;
                }

                results.push((c_id, global_idx, msg.t));
            }
        }
    }

    results.sort_by(|a, b| b.2.cmp(&a.2));

    let mut search_state = SEARCH_RESULTS.lock().unwrap();
    *search_state = results.into_iter().map(|(c, idx, _)| (c, idx)).collect();
    
    search_state.len()
}

#[wasm_bindgen]
pub fn get_search_results_html(start_idx: usize, end_idx: usize) -> String {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();
    let search_state = SEARCH_RESULTS.lock().unwrap();

    if search_state.is_empty() || start_idx >= search_state.len() {
        return String::new();
    }

    let end = std::cmp::min(end_idx + 1, search_state.len());
    let mut html = String::with_capacity((end - start_idx) * 350);
    
    for (c_id, global_idx) in &search_state[start_idx..end] {
        let msgs = data.messages.get(c_id).unwrap();
        let msg = &msgs[*global_idx];
        let user = data.users.get(&msg.a).unwrap();
        let ch_name = data.channels.iter().find(|c| c.id == *c_id).map(|c| c.n.as_ref()).unwrap_or("Unknown");
        
        let safe_content = if msg.c.trim().is_empty() {
            "<span style=\"color: var(--text-muted); font-style: italic;\">[attachment]</span>".to_string()
        } else {
            escape_html(&msg.c)
        };
        let safe_name = escape_html(&user.n);
        let safe_username = escape_html(&user.u);
        let color = user.c.as_deref().unwrap_or("#ffffff");

        let avatar_html = if let Some(pfp) = &user.p {
            format!("<div class=\"search-result-avatar avatar-wrapper\"><img src=\"__DEFAULT_PFP__\" class=\"avatar-layer\"><img src=\"{}\" loading=\"lazy\" class=\"avatar-layer\" style=\"opacity: 0;\" onload=\"this.style.opacity=1; this.previousElementSibling.style.display='none';\"></div>", pfp)
        } else {
            "<img src=\"__DEFAULT_PFP__\" class=\"search-result-avatar\">".to_string()
        };

        html.push_str(&format!(
            "<div class=\"search-result-item\" onclick=\"jumpToMessage('{}', {})\">
                <div class=\"search-result-header\">
                    {}
                    <div class=\"search-result-meta\">
                        <span class=\"search-result-name\" style=\"color: {}\" title=\"{}\">{}</span>
                        <span class=\"search-result-time ts-full\" data-ts=\"{}\"></span>
                    </div>
                </div>
                <div class=\"search-result-text\">{}</div>
                <div class=\"search-result-channel\"># {}</div>
            </div>",
            c_id, global_idx, avatar_html, color, safe_username, safe_name, msg.t, safe_content, escape_html(ch_name)
        ));
    }
    html
}

#[wasm_bindgen]
pub fn export_search_results_text() -> String {
    let archive = ARCHIVE.lock().unwrap();
    let data = archive.as_ref().unwrap();
    let search_state = SEARCH_RESULTS.lock().unwrap();

    if search_state.is_empty() {
        return String::new();
    }

    let mut export_string = String::with_capacity(search_state.len() * 128);

    for (c_id, global_idx) in search_state.iter().rev() {
        if let Some(msgs) = data.messages.get(c_id) {
            if let Some(msg) = msgs.get(*global_idx) {
                if let Some(user) = data.users.get(&msg.a) {
                    let datetime = Utc.timestamp_millis_opt(msg.t).unwrap();
                    let formatted_time = datetime.format("%m/%d/%y %H:%M").to_string();
                    
                    let content = if msg.c.trim().is_empty() {
                        "[attachment]"
                    } else {
                        &msg.c
                    };

                    export_string.push_str(&format!(
                        "{} {}: {}\n",
                        formatted_time,
                        user.n,
                        content
                    ));
                }
            }
        }
    }

    export_string
}
