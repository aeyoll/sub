use crate::lib::episode::Episode;
use crate::lib::providers::HttpProvider;
use crate::lib::show::Show;
use crate::lib::subtitle::Subtitle;
use crate::{File, Lang};
use anyhow::{anyhow, Error};
use serde::Deserialize;
use std::env;
use std::time::Duration;
use ureq::{Agent, Request};

const OPEN_SUBTITLES_API_KEY_HEADER: &str = "Api-Key";

#[derive(Debug, Deserialize)]
struct OpenSubtitleDownloadResponse {
    pub link: String,
}

#[derive(Debug, Deserialize)]
struct OpenSubtitleSubtitleResponseDataAttributeFile {
    pub file_id: i32,
}

#[derive(Debug, Deserialize)]
struct OpenSubtitleSubtitleResponseDataAttributeFeatureDetail {
    pub feature_id: i32,
    pub title: String,
    pub season_number: i32,
    pub episode_number: i32,
    pub parent_title: String,
}

#[derive(Debug, Deserialize)]
struct OpenSubtitleSubtitleResponseDataAttribute {
    // pub subtitle_id: String,
    pub download_count: i32,
    pub feature_details: OpenSubtitleSubtitleResponseDataAttributeFeatureDetail,
    pub files: Vec<OpenSubtitleSubtitleResponseDataAttributeFile>,
    pub upload_date: String,
}

#[derive(Debug, Deserialize)]
struct OpenSubtitleSubtitleResponseData {
    #[serde(rename = "type")]
    pub data_type: String,
    pub attributes: OpenSubtitleSubtitleResponseDataAttribute,
}

#[derive(Debug, Deserialize)]
struct OpenSubtitleSubtitleResponse {
    pub data: Vec<OpenSubtitleSubtitleResponseData>,
}

pub struct OpenSubtitleProvider {
    file: File,
    api_url: String,
    api_key: String,
}

impl OpenSubtitleProvider {
    pub fn new(file: File) -> Result<Self, Error> {
        let api_key = match env::var("OPEN_SUBTITLES_API_KEY") {
            Ok(beta_series_api_key) => beta_series_api_key,
            Err(_) => {
                return Err(anyhow!(
                    "Please set a OPEN_SUBTITLES_API_KEY environment variable"
                ))
            }
        };

        Ok(OpenSubtitleProvider {
            file,
            api_url: String::from("https://api.opensubtitles.com/api/v1/"),
            api_key,
        })
    }

    fn get_agent(&self) -> Agent {
        ureq::AgentBuilder::new()
            .timeout_read(Duration::from_secs(5))
            .timeout_write(Duration::from_secs(5))
            .build()
    }

    pub fn get(&self, url: String) -> Request {
        self.get_agent()
            .get(&url)
            .set(OPEN_SUBTITLES_API_KEY_HEADER, &self.api_key)
    }

    pub fn post(&self, url: String) -> Request {
        self.get_agent()
            .post(&url)
            .set(OPEN_SUBTITLES_API_KEY_HEADER, &self.api_key)
    }
}

impl HttpProvider for OpenSubtitleProvider {
    fn name(&self) -> &str {
        "OpenSubtitles"
    }

    fn get_lang(&self, lang: Lang) -> Result<String, Error> {
        Ok(lang.code)
    }

    fn get_query(&self) -> Result<String, Error> {
        let (hash, _) = self.file.get_hash();
        Ok(hash)
    }

    fn search_subtitle(&self, lang: Lang) -> Result<(Episode, Subtitle), Error> {
        let language = self.get_lang(lang)?;
        let filename = self.file.get_filename().to_string_lossy().to_string();
        let query = self.get_query()?;
        let qs = querystring::stringify(vec![
            ("query", filename.as_ref()),
            ("languages", language.as_ref()),
            ("moviehash", query.as_ref()),
        ]);

        let url = format!("{}subtitles?{}", self.api_url, qs);
        let request = self.get(url);
        // let response = request.clone().call()?.into_string();
        // println!("{:?}", response);
        let response: OpenSubtitleSubtitleResponse = request.call()?.into_json()?;

        if response.data.is_empty() {
            return Err(anyhow!("Received empty data from OpenSubtitles"));
        }

        let most_downloaded_subtitle = response
            .data
            .iter()
            .filter(|d| d.data_type == "subtitle")
            .max_by_key(|d| d.attributes.download_count)
            .unwrap();

        let attributes = &most_downloaded_subtitle.attributes;
        let feature_details = &attributes.feature_details;
        let code = format!(
            "S{}E{}",
            feature_details.season_number, feature_details.episode_number
        );

        let episode = Episode {
            id: feature_details.feature_id,
            title: feature_details.title.to_string(),
            season: feature_details.season_number,
            episode: feature_details.episode_number,
            code,
            description: "".to_string(),
            date: "".to_string(),
            subtitles: vec![],
            show: Show {
                id: 0,
                title: feature_details.parent_title.to_string(),
            },
        };
        let subtitle = Subtitle {
            id: attributes.files.first().unwrap().file_id,
            language,
            source: "".to_string(),
            quality: 0,
            file: "".to_string(),
            url: "".to_string(),
            date: attributes.upload_date.to_string(),
        };

        Ok((episode, subtitle))
    }

    fn download_subtitle(&self, subtitle: Subtitle) -> Result<String, Error> {
        let url = format!("{}download", self.api_url);
        let data = ureq::json!({
            "file_id": subtitle.id
        });

        let request = self.post(url);
        let link = match request.send_json(data) {
            Ok(response) => {
                let response: OpenSubtitleDownloadResponse = response.into_json()?;
                response.link
            }
            Err(e) => {
                println!("{:?}", e);
                return Err(anyhow!("KO"));
            }
        };

        let content = self.get(link).call().unwrap().into_string().unwrap();

        Ok(content)
    }

    fn write_subtitle(&self, contents: String) -> Result<(), Error> {
        self.file.download(contents)?;

        Ok(())
    }
}
