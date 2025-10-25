use std::{
    collections::{BTreeMap, HashMap},
    env,
    sync::{LazyLock, RwLock},
};

use serde::Deserialize;
use twilight_model::{
    application::{
        command::{Command, CommandOption, CommandType},
        interaction::{
            InteractionContextType, InteractionData, application_command::CommandOptionValue,
        },
    },
    channel::message::Embed,
    gateway::payload::incoming::{InteractionCreate, MessageCreate},
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    id::{
        Id,
        marker::{ChannelMarker, GuildMarker, UserMarker},
    },
    oauth::ApplicationIntegrationType,
};

mod http;
use http::HttpClient;

// Use a procedural macro to generate bindings for the world we specified in
// `../wit/world.wit`
wit_bindgen::generate!();

use crate::{
    discord_bot::plugin::{
        discord_types::Requests as DiscordRequests,
        plugin_types::RegistrationsResponseDiscordEvents,
    },
    exports::discord_bot::plugin::plugin_functions::{
        DiscordEvents, Guest, RegistrationsResponse, SupportedRegistrations,
    },
};

// Define a custom srtuct and implement the generated `Guest` trait for it which
// represents implementing all the necessary exported interfaces for this
// component. This type can also store plugin context.
struct Plugin {
    http_client: HttpClient,
    storred_settings: RwLock<PluginStoredSettings>,
    stats: RwLock<PluginStats>,
}

struct PluginStoredSettings {
    cat_message_response_chance: u8,
    automated_cats: Vec<PluginStoredSettingsAutomatedCat>,
    show_error_embeds: bool,
}

struct PluginStoredSettingsAutomatedCat {
    id: u16,
    guild: Id<GuildMarker>,
    channel: Id<ChannelMarker>,
}

struct PluginStats {
    total_cats_requested: u32,
    cat_messages_detected: u32,
    cats_on_demand: u32,
    automated_cats: u32,
    most_cats_demanded: BTreeMap<Id<UserMarker>, u32>,
}

// FIXME: Does not actually work with no settings
#[derive(Debug, Deserialize)]
struct PluginSettings {
    #[serde(default = "PluginSettings::cat_message_response_chance_default")]
    cat_message_response_chance: u8,
    #[serde(default = "PluginSettings::cats_on_demand_default")]
    cats_on_demand: bool,
    #[serde(default = "PluginSettings::automated_cats_default")]
    automated_cats: Vec<PluginSettingsAutomatedCat>,
    #[serde(default = "PluginSettings::show_error_embeds_default")]
    show_error_embeds: bool,
}

#[derive(Debug, Deserialize)]
struct PluginSettingsAutomatedCat {
    guild_id: Id<GuildMarker>,
    channel_id: Id<ChannelMarker>,
    cron: String,
}

impl PluginSettings {
    fn cat_message_response_chance_default() -> u8 {
        0
    }

    fn cats_on_demand_default() -> bool {
        true
    }

    fn automated_cats_default() -> Vec<PluginSettingsAutomatedCat> {
        vec![]
    }

    fn show_error_embeds_default() -> bool {
        true
    }
}

static CONTEXT: LazyLock<Plugin> = LazyLock::new(|| Plugin {
    http_client: HttpClient::new(),
    storred_settings: RwLock::new(PluginStoredSettings {
        cat_message_response_chance: 0,
        automated_cats: vec![],
        show_error_embeds: true,
    }),
    stats: RwLock::new(PluginStats {
        total_cats_requested: 0,
        cat_messages_detected: 0,
        cats_on_demand: 0,
        automated_cats: 0,
        most_cats_demanded: BTreeMap::new(),
    }),
});

impl Guest for Plugin {
    fn registrations(
        mut settings: Vec<u8>,
        supported_registrations: SupportedRegistrations,
    ) -> Result<RegistrationsResponse, String> {
        if env::var("API_KEY").is_err() {
            return Err(String::from(
                "The API_KEY environment variable was not provided",
            ));
        }

        let settings = match simd_json::from_slice::<PluginSettings>(&mut settings) {
            Ok(settings) => settings,
            Err(err) => {
                return Err(format!(
                    "The provided settings were of the incorrect structure, error: {}",
                    &err,
                ));
            }
        };

        // Write settings to CONTEXT and use that.

        let mut commands = vec![];

        if supported_registrations.discord_events.interaction_create && settings.cats_on_demand {
            commands.push((
                String::from("cat"),
                simd_json::to_vec(&Command {
                    application_id: None,
                    contexts: Some(vec![
                        InteractionContextType::Guild,
                        InteractionContextType::BotDm,
                        InteractionContextType::PrivateChannel,
                    ]),
                    default_member_permissions: None,
                    #[allow(deprecated)]
                    dm_permission: None,
                    description: String::from("Request a cat"),
                    description_localizations: None,
                    guild_id: None,
                    id: None,
                    integration_types: Some(vec![
                        ApplicationIntegrationType::GuildInstall,
                        ApplicationIntegrationType::UserInstall,
                    ]),
                    kind: CommandType::ChatInput,
                    name: String::from("cat"),
                    name_localizations: None,
                    nsfw: Some(false),
                    options: vec![CommandOption {
                        autocomplete: None,
                        channel_types: None,
                        choices: None,
                        description: String::from("The ID of the requested cat"),
                        description_localizations: None,
                        kind: twilight_model::application::command::CommandOptionType::String,
                        max_length: None,
                        max_value: None,
                        min_length: None,
                        min_value: None,
                        name: String::from("id"),
                        name_localizations: None,
                        options: None,
                        required: Some(false),
                    }],
                    version: Id::new(1),
                })
                .unwrap(),
            ));
        }

        let mut scheduled_jobs = HashMap::new();

        for automated_cat in settings.automated_cats {
            // TODO: Write successful items to CONTEXT
            scheduled_jobs
                .entry(format!(
                    "automated_cat_{}_{}",
                    &automated_cat.guild_id, &automated_cat.channel_id
                ))
                .or_insert(vec![])
                .push(automated_cat.cron);
        }

        Ok(RegistrationsResponse {
            discord_events: RegistrationsResponseDiscordEvents {
                interaction_create_commands: commands,
                message_create: supported_registrations.discord_events.message_create
                    && settings.cat_message_response_chance != 0,
                thread_create: false,
                thread_delete: false,
                thread_list_sync: false,
                thread_member_update: false,
                thread_members_update: false,
                thread_update: false,
            },
            scheduled_jobs: scheduled_jobs.into_iter().collect(),
            dependency_functions: vec![String::from("request_cat")],
        })
    }

    fn shutdown() -> Result<(), _rt::String> {
        todo!();
    }

    fn discord_event(event: DiscordEvents) -> Result<(), String> {
        match event {
            DiscordEvents::InteractionCreate(mut interaction) => {
                let interaction =
                    Box::new(simd_json::from_slice::<InteractionCreate>(&mut interaction).unwrap());

                match interaction.data.as_ref() {
                    Some(InteractionData::ApplicationCommand(command_data)) => {
                        match command_data.name.as_str() {
                            "cat" => CONTEXT.cat_command(interaction),
                            &_ => unimplemented!(),
                        }
                    }
                    _ => unimplemented!(),
                }
            }
            DiscordEvents::MessageCreate(mut message) => {
                let message =
                    Box::new(simd_json::from_slice::<MessageCreate>(&mut message).unwrap());

                if message.0.content.to_lowercase().contains("cat") {
                    return CONTEXT.cat_message(message);
                }
                Ok(())
            }
            _ => unimplemented!(),
        }
    }

    fn scheduled_job(job: String) -> Result<(), String> {
        match job.as_str() {
            "automated_cat" => CONTEXT.automated_cat(),
            &_ => unimplemented!(),
        }
    }

    fn dependency(function: String, params: Vec<u8>) -> Result<Vec<u8>, String> {
        todo!();
    }
}

impl Plugin {
    fn cat_command(&self, mut interaction: Box<InteractionCreate>) -> Result<(), String> {
        let mut discord_requests = vec![];

        match interaction.data.as_mut().unwrap() {
            InteractionData::ApplicationCommand(command_data) => {
                command_data.options.reverse();

                let id = match command_data.options.pop() {
                    Some(id) => match id.value {
                        CommandOptionValue::String(id) => Some(id),
                        _ => None,
                    },
                    None => None,
                };

                match self.http_client.request_cat(id) {
                    Ok(mut cat_response) => {
                        let interaction_response_data = InteractionResponseData {
                            allowed_mentions: None,
                            attachments: None,
                            choices: None,
                            components: None,
                            content: Some(cat_response.pop().unwrap().url),
                            custom_id: None,
                            embeds: None,
                            flags: None,
                            title: None,
                            tts: None,
                        };

                        let interaction_response = InteractionResponse {
                            kind: InteractionResponseType::ChannelMessageWithSource,
                            data: Some(interaction_response_data),
                        };

                        discord_requests.push(DiscordRequests::InteractionCallback((
                            interaction.id.get(),
                            interaction.token.clone(),
                            simd_json::to_vec(&interaction_response).unwrap(),
                        )));
                    }
                    Err(err) => {
                        // TODO: Use an embed

                        //let embed = Self::create_error_embed(err);

                        let interaction_response_data = InteractionResponseData {
                            allowed_mentions: None,
                            attachments: None,
                            choices: None,
                            components: None,
                            content: Some(err),
                            custom_id: None,
                            embeds: None,
                            flags: None,
                            title: None,
                            tts: None,
                        };

                        let interaction_response = InteractionResponse {
                            kind: InteractionResponseType::ChannelMessageWithSource,
                            data: Some(interaction_response_data),
                        };

                        discord_requests.push(DiscordRequests::InteractionCallback((
                            interaction.id.get(),
                            interaction.token.clone(),
                            simd_json::to_vec(&interaction_response).unwrap(),
                        )));
                    }
                };
            }
            _ => unreachable!(),
        };

        Ok(())
    }

    fn cat_message(&self, message: Box<MessageCreate>) -> Result<(), String> {
        todo!()
    }

    fn automated_cat(&self) -> Result<(), String> {
        todo!();
    }

    fn create_error_embed<T: AsRef<str>>(message: T) -> Embed {
        todo!();
    }
}

// export! defines that the `Plugin` struct defined below is going to define
// the exports of the `plugin` world, namely the `init` and `cleanup` function.
export!(Plugin);
