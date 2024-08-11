use std::{env,fs,io::Write,path::{self, Path},collections::HashMap};
use serde_json;
use serde::Deserialize;
use linkify::{LinkFinder,LinkKind};
use url_normalizer;
use url::Url;
use reqwest::{self, header::USER_AGENT};


#[allow(dead_code)]
#[derive(Deserialize)]
struct ChannelData{
    id: String,
    r#type: u32,
    name: Option<String>,
    recipients: Option<Vec<String>>
}

#[allow(non_snake_case)]
#[derive(Deserialize)]
struct ChannelMessages{
    Contents: String,
    Attachments: String
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() < 4 {
        println!("Usage: program <channel_id> <output_folder> <cdn_bypass_server_adress>");
        return std::process::ExitCode::FAILURE
    }

    println!("Discord CDN bypass server: \"{}\"",args[3]);
    println!("Output Directory: \"{}\"\n",       args[2]);

    let channel_id = String::from("./messages/c") + &args[1];

    let channel_meta = fs::read_to_string(
        path::Path::new(&channel_id).join("channel.json")
    ).expect("You moron, there is no such channel in the messages folder\n");

    let channel_messages = fs::read_to_string(
        path::Path::new(&channel_id).join("messages.json")
    ).unwrap();

    let meta:     ChannelData          = serde_json::from_str(&channel_meta)    .unwrap();
    let messages: Vec<ChannelMessages> = serde_json::from_str(&channel_messages).unwrap();

    display_channel_meta(meta);
    let matched_links: Vec<Url> = find_links(messages);
    println!("  - {} unique URLs found",matched_links.len());
    let media_links = filter_media_links(matched_links);
    println!("  - {} unique media links found\n",media_links.len());
    download_media_links(media_links,&args[2],&args[3]);
    println!("Finished downloading media.");

    std::process::ExitCode::SUCCESS
}

fn find_links(messages: Vec<ChannelMessages>) -> Vec<Url> {
    let mut matched_links: Vec<Url> = Vec::new();

    let link_matcher = LinkFinder::new();

    println!("\nMatching URLs..");

    for msg in &messages {
        for link in link_matcher.links(&msg.Contents) {
            if link.kind() == &LinkKind::Url {
                matched_links.push(url_normalizer::normalize(
                    Url::parse(
                        link.as_str()
                    ).unwrap()
                ).unwrap());
            }
        }

        for link in link_matcher.links(&msg.Attachments) {

            if link.kind() == &LinkKind::Url {
                matched_links.push(url_normalizer::normalize(
                    Url::parse(
                        link.as_str()
                    ).unwrap()
                ).unwrap());
            }
        }
    }

    println!("  - {} links found",matched_links.len());
    println!("Normalizing URLs..");

    let mut unique_urls = HashMap::new();

    matched_links.iter().for_each(|url| {
        let key = format!(
            "{}{}{}",
            url.host_str().unwrap_or(""),
            url.domain().unwrap_or(""),
            url.path()
        );
        unique_urls.insert(key,url.clone());
    });

    unique_urls.values().cloned().collect()
}

fn filter_media_links(links: Vec<Url>) -> Vec<Url> {
    let mut result: Vec<Url> = Vec::new();

    println!("Filtering media links..");

    for link in links.iter() {
        let path:  String = link.path().to_lowercase();

        let media: bool = path.ends_with("mp4")
        || path.ends_with("png")
        || path.ends_with("jpg")
        || path.ends_with("gif");

        if media {
            result.push(link.to_owned());
        }
    }

    result
}

fn create_unique_file(path: &Path, filename: &str) -> fs::File {
    let (name, ext) = match filename.rsplit_once('.') {
        Some((name, ext)) => (name.to_string(), ext.to_string()),
        None => (filename.to_string(), String::new()),
    };

    let mut final_path = path.join(format!("{}.{}", name, ext));
    let mut counter = 1;

    while final_path.exists() {
        final_path = path.join(format!("{}_{}.{ext}", name, counter));
        counter += 1;
    }

    println!("Saving file as: {}",final_path.display());

    let file = fs::File::create(&final_path).expect("\nOutput directory probably doesnt exist!!\n\n");

    file
}

fn truncate_filename(filename: &str, max_length: usize) -> String {
    let extension = path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    let base_length = filename.len().saturating_sub(extension.len()+1);

    if base_length > max_length {
        let new_base_length = max_length.saturating_sub(extension.len()+1);
        let truncated_base   = &filename[0..new_base_length];
        format!("{}.{extension}",truncated_base)
    } else {
        filename.to_string()
    }
}

const MAX_FILENAME_LENGTH: usize = 100;
fn download_media_links(links: Vec<Url>,output_folder_path: &str,bypass_server: &str) {
    let mut i: u32 = 0;

    let client   = reqwest::blocking::Client::new();
    let user_agent = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) discord/0.0.52 Chrome/120.0.6099.291 Electron/28.2.10 Safari/537.36";

    for link in links.iter() {
        i += 1;
        println!("({}/{}) Downloading media from: \"{}\"",i,links.len(),link.as_str());

        let mut response = match client.get(link.as_str())
            .header(USER_AGENT,user_agent)
            .send() {
                Ok(r) => r,
                Err(e) => {
                    println!("Error fetching URL {}: {:?}",link.as_str(),e);
                    continue;
                }
            };

        let mut bytes = Vec::new();
        if let Err(e) = response.copy_to(&mut bytes) {
            println!("Error reading response body for {}: {:?}", link.as_str(), e);
            continue;
        }

        let body_str = String::from_utf8_lossy(&bytes);
        if body_str.contains("This content is no longer available") {
            println!("Content unavailable at {}, using bypass server {}",link.as_str(),bypass_server);
            let new_url = format!("{}{}",bypass_server,link.path());
            response = match client.get(&new_url)
                .header(USER_AGENT,user_agent)
                .send() {
                    Ok(r) => r,
                    Err(e) => {
                        println!("Error fetching bypass server URL {}: {:?}",new_url,e);
                        continue;
                    }
                };

            bytes.clear();
            if let Err(e) = response.copy_to(&mut bytes) {
                println!("Error reading response body for bypass server URL {}: {:?}",new_url,e);
                continue;
            }
        }

        let filename = link.path_segments()
            .and_then(|segments| segments.last())
            .unwrap_or("default_filename");

        let filename = truncate_filename(filename,MAX_FILENAME_LENGTH);

        let output_path = path::Path::new(output_folder_path);
        let mut file = create_unique_file(output_path,&filename);

        if let Err(e) = file.write_all(&bytes) {
            println!("Error writing data to file {}: {:?}",filename,e);
            continue;
        }

        println!("Successfully saved file: {}\n",filename);
    }
}

fn display_channel_meta(data: ChannelData) {
    println!("Loading data for channel \"{}\"",data.name.unwrap_or(String::from("Unknown")));
    println!("  - Channel type: {} ({})",data.r#type,get_channel_type(data.r#type));
    println!("  - Channel ID:   {}",data.id);
    if data.recipients.is_some() {
        println!("  - Recipients:");
        for recipient in data.recipients.unwrap() {
            println!("    - {}",recipient);
        }
    }
}

fn get_channel_type(channel_type: u32) -> String {
    match channel_type {
        0  => String::from("GuildtextChat"),
        1  => String::from("DirectTextChat"),
        2  => String::from("GuildVoiceChat"),
        3  => String::from("DirectGroupTextChat"),
        4  => String::from("GuildCategory"),
        5  => String::from("GuildNews"),
        10 => String::from("GuildNewsThread"),
        11 => String::from("GuildPublicThread"),
        12 => String::from("GuildPrivateThread"),
        13 => String::from("GuildStageVoice"),
        14 => String::from("GuildDirectory"),
        15 => String::from("GuildForum"),
        _  => String::from("Unknown"),
    }
}