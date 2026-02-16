use std::env;

use serde::Deserialize;

use wstd::{
    http::{Client, Request},
    runtime::block_on,
};

use crate::CONTEXT;

pub struct HttpClient {
    client: Client,
}

// API response types
// TODO: Complete
#[derive(Deserialize)]
#[allow(dead_code)]
pub struct CatResponse {
    pub id: String,
    pub url: String,
    pub width: Option<u16>,
    pub heigth: Option<u16>,
    pub mime_type: Option<String>,
    //breeds: Vec<CatResponseBreed>,
    //favourite: Option<CatResponseFavourite>,
}

//#[derive(Deserialize)]
//#[allow(dead_code)]
//struct CatResponseBreed {
//    weight: CatResponseBreedWeight,
//    id: String,
//    name: String,
//    cfa_url: String,
//    vetstreet_url: String,
//    vcahospitals_url: String,
//    temperament: String,
//    origin: String,
//    country_codes: String,
//    country_code: String,
//    description: String,
//    life_span: String,
//    indoor: u8,
//    lap: u8,
//    alt_names: String,
//    adaptability: u8,
//    affection_level: u8,
//    child_friendly: u8,
//    dog_friendly: u8,
//    energy_level: u8,
//    grooming: u8,
//    health_issues: u8,
//    intelligence: u8,
//    shedding_level: u8,
//    social_needs: u8,
//    stranger_friendly: u8,
//    vocalisation: u8,
//    experimental: u8,
//    hairless: u8,
//    natural: u8,
//    rare: u8,
//    rex: u8,
//    suppressed_tail: u8,
//    short_legs: u8,
//    wikipedia_url: String,
//    hypoallergenic: u8,
//    reference_image_id: String,
//}

//#[derive(Deserialize)]
//#[allow(dead_code)]
//struct CatResponseBreedWeight {
//    imperial: String,
//    metric: String,
//}
//
//#[derive(Deserialize)]
//#[allow(dead_code)]
//struct CatResponseFavourite {}

impl HttpClient {
    pub fn new() -> Self {
        HttpClient {
            client: Client::new(),
        }
    }

    pub fn request_cat(&self, id: Option<String>) -> Result<Vec<CatResponse>, String> {
        CONTEXT.stats.write().unwrap().total_cats_requested += 1;

        let mut uri = String::from("https://api.thecatapi.com/v1/images/");

        match id {
            Some(id) => uri.push_str(&id),
            None => uri.push_str("search"),
        }

        let request = match Request::get(&uri)
            .header("x-api-key", env::var("API_KEY").unwrap())
            .body(())
        {
            Ok(request) => request,
            Err(err) => {
                return Err(format!(
                    "An error occured while building the HTTP request: {err}"
                ));
            }
        };

        let mut response = match block_on(self.client.send(request)) {
            Ok(response) => response,
            Err(err) => {
                return Err(format!(
                    "An error occured while making the HTTP request: {err}"
                ));
            }
        };

        if response.status() != 200 {
            return Err(format!(
                "The HTTP response returned an unwanted status code: {}",
                response.status(),
            ));
        }

        let response_body = match block_on(response.body_mut().contents()) {
            Ok(response_body) => response_body,
            Err(err) => {
                return Err(format!(
                    "An error occured while getting the HTTP response body, error: {}",
                    &err,
                ));
            }
        };

        match sonic_rs::from_slice::<Vec<CatResponse>>(response_body) {
            Ok(cat_responses) => Ok(cat_responses),
            Err(err) => Err(format!(
                "An error occured while deserializing the HTTP response body, error: {}",
                &err,
            )),
        }
    }
}
