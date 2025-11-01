use std::sync::{LazyLock, RwLock};

use serde::Deserialize;
use twilight_model::{
    application::{
        command::{Command, CommandType},
        interaction::{InteractionContextType, InteractionData},
    },
    channel::message::{
        AllowedMentions, Component, MentionType,
        component::{TextInput, TextInputStyle},
    },
    gateway::payload::incoming::InteractionCreate,
    guild::Permissions,
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    id::Id,
    oauth::ApplicationIntegrationType,
};

wit_bindgen::generate!();

use crate::{
    discord_bot::plugin::{
        host_functions::discord_request, plugin_types::RegistrationsResponseDiscordEvents,
    },
    exports::discord_bot::plugin::plugin_functions::{
        DiscordEvents, Guest, RegistrationsResponse, SupportedRegistrations,
    },
};

struct Plugin {
    storred_settings: RwLock<PluginStoredSettings>,
}

struct PluginStoredSettings {
    extension_requests_channel_id: u64,
}

#[derive(Deserialize)]
struct PluginSettings {
    extension_requests_channel_id: u64,
}

static CONTEXT: LazyLock<Plugin> = LazyLock::new(|| Plugin {
    storred_settings: RwLock::new(PluginStoredSettings {
        extension_requests_channel_id: 0,
    }),
});

impl Guest for Plugin {
    fn registrations(
        mut settings: Vec<u8>,
        supported_registrations: SupportedRegistrations,
    ) -> Result<RegistrationsResponse, String> {
        if !supported_registrations.discord_events.interaction_create {
            return Err(String::from(
                "This plugin requires the interactionCreate event to be enabled.",
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

        // TODO: Get channel to its guild id and verify its existence

        CONTEXT
            .storred_settings
            .write()
            .unwrap()
            .extension_requests_channel_id = settings.extension_requests_channel_id;

        let commands = vec![(
            String::from("request-extension"),
            simd_json::to_vec(&Command {
                application_id: None,
                contexts: Some(vec![InteractionContextType::Guild]),
                default_member_permissions: Some(Permissions::SEND_MESSAGES),
                #[allow(deprecated)]
                dm_permission: None,
                description: String::from("Request a Paperback extension"),
                description_localizations: None,
                guild_id: None,
                id: None,
                integration_types: Some(vec![
                    ApplicationIntegrationType::GuildInstall,
                    ApplicationIntegrationType::UserInstall,
                ]),
                kind: CommandType::ChatInput,
                name: String::from("request-extension"),
                name_localizations: None,
                nsfw: Some(false),
                options: vec![],
                version: Id::new(1),
            })
            .unwrap(),
        )];

        Ok(RegistrationsResponse {
            discord_events: RegistrationsResponseDiscordEvents {
                interaction_create_commands: commands,
                message_create: false,
                thread_create: false,
                thread_delete: false,
                thread_list_sync: false,
                thread_member_update: false,
                thread_members_update: false,
                thread_update: false,
            },
            scheduled_jobs: vec![],
            dependency_functions: vec![],
        })
    }

    fn shutdown() -> Result<(), _rt::String> {
        todo!();
    }

    fn discord_event(event: DiscordEvents) -> Result<(), String> {
        match event {
            DiscordEvents::InteractionCreate(mut interaction_create) => {
                let interaction_create =
                    simd_json::from_slice::<InteractionCreate>(&mut interaction_create).unwrap();

                match interaction_create.data.as_ref() {
                    Some(InteractionData::ApplicationCommand(command_data)) => {
                        match command_data.name.as_str() {
                            "request-extension" => {
                                Plugin::extension_request_modal(&interaction_create);
                                Ok(())
                            }
                            &_ => unimplemented!(),
                        }
                    }
                    _ => unimplemented!(),
                }
            }
            _ => unimplemented!(),
        }
    }

    fn scheduled_job(_job: String) -> Result<(), String> {
        unimplemented!();
    }

    fn dependency(_function: String, _params: Vec<u8>) -> Result<Vec<u8>, String> {
        unimplemented!();
    }
}

impl Plugin {
    fn extension_request_modal(interaction_create: &InteractionCreate) {
        let modal = InteractionResponse {
            kind: InteractionResponseType::Modal,
            data: Some(InteractionResponseData {
                allowed_mentions: Some(AllowedMentions {
                    parse: vec![MentionType::Users],
                    replied_user: true,
                    roles: vec![],
                    users: vec![],
                }),
                attachments: None,
                choices: None,
                components: Some(vec![Component::TextInput(TextInput {
                    custom_id: String::from("test"),
                    label: String::from("Test"),
                    max_length: None,
                    min_length: None,
                    placeholder: Some(String::from("This is a test")),
                    required: Some(true),
                    style: TextInputStyle::Short,
                    value: None,
                })]),
                content: None,
                custom_id: None,
                embeds: None,
                flags: None,
                title: Some(String::from("Extension Request")),
                tts: None,
            }),
        };

        let _ = discord_request(
            &discord_bot::plugin::discord_types::Requests::InteractionCallback((
                interaction_create.id.get(),
                interaction_create.token.clone(),
                simd_json::to_vec(&modal).unwrap(),
            )),
        );
    }
}

export!(Plugin);
