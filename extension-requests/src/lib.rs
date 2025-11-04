use std::{
    sync::{LazyLock, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use twilight_model::{
    application::{
        command::{Command, CommandType},
        interaction::{InteractionContextType, InteractionData},
    },
    channel::{
        Channel, ChannelType,
        message::{
            AllowedMentions, Component, Embed, MentionType, MessageFlags,
            component::{ActionRow, TextInput, TextInputStyle},
            embed::EmbedFooter,
        },
        thread::ThreadsListing,
    },
    gateway::payload::incoming::InteractionCreate,
    guild::Permissions,
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    id::Id,
    oauth::ApplicationIntegrationType,
    util::Timestamp,
};
use url::Url;

wit_bindgen::generate!();

use crate::{
    discord_bot::plugin::{
        discord_types::Requests,
        host_functions::discord_request,
        plugin_types::{
            RegistrationsResponseDiscordEvents, RegistrationsResponseDiscordEventsInteractionCreate,
        },
    },
    exports::discord_bot::plugin::plugin_functions::{
        DiscordEvents, Guest, RegistrationsResponse, SupportedRegistrations,
    },
};

struct Plugin {
    storred_settings: RwLock<PluginSettings>,
}

#[derive(Deserialize)]
struct PluginSettings {
    extension_requests_channel_id: u64,
    tags: PluginSettingsTags,
}

#[derive(Deserialize)]
struct PluginSettingsTags {
    creation: Vec<u64>,
}

#[derive(Serialize)]
struct CreateMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_mentions: Option<AllowedMentions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embeds: Option<Vec<Embed>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    flags: Option<MessageFlags>,
}

#[derive(Serialize)]
struct CreateForumThread {
    name: String,
    message: CreateMessage,
    applied_tags: Vec<u64>,
}

static CONTEXT: LazyLock<Plugin> = LazyLock::new(|| Plugin {
    storred_settings: RwLock::new(PluginSettings {
        extension_requests_channel_id: 0,
        tags: PluginSettingsTags { creation: vec![] },
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

        let mut get_channel_response = discord_request(&Requests::GetChannel(
            settings.extension_requests_channel_id,
        ))?
        .unwrap();

        let channel = match simd_json::from_slice::<Channel>(&mut get_channel_response) {
            Ok(channel) => channel,
            Err(err) => {
                return Err(format!(
                    "Something went wrong while deserializing the response from Discord, error {}",
                    &err
                ));
            }
        };

        if channel.kind != ChannelType::GuildForum {
            return Err(String::from(
                "The provided requests channel needs to be of the forum type.",
            ));
        }

        for creation_tag in settings.tags.creation {
            if channel
                .available_tags
                .as_ref()
                .unwrap()
                .iter()
                .any(|forum_tag| forum_tag.id == creation_tag)
            {
                CONTEXT
                    .storred_settings
                    .write()
                    .unwrap()
                    .tags
                    .creation
                    .push(creation_tag);
            }
        }

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
                guild_id: Some(channel.guild_id.unwrap()),
                id: None,
                integration_types: Some(vec![ApplicationIntegrationType::GuildInstall]),
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
                interaction_create: RegistrationsResponseDiscordEventsInteractionCreate {
                    application_commands: commands,
                    message_components: vec![],
                    modals: vec![String::from("extension-request")],
                },
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
        Ok(())
    }

    fn discord_event(event: DiscordEvents) -> Result<(), String> {
        match event {
            DiscordEvents::InteractionCreate(mut interaction_create) => {
                let interaction_create =
                    simd_json::from_slice::<InteractionCreate>(&mut interaction_create).unwrap();

                match interaction_create.data.as_ref() {
                    Some(InteractionData::ApplicationCommand(command_data)) => {
                        match command_data.name.as_str() {
                            "request-extension" => Plugin::request_extension(&interaction_create),
                            &_ => unimplemented!(),
                        }
                    }
                    Some(InteractionData::ModalSubmit(modal_interaction_data)) => {
                        match modal_interaction_data.custom_id.as_str() {
                            "extension-request" => Plugin::extension_request(&interaction_create),
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
    fn request_extension(interaction_create: &InteractionCreate) -> Result<(), String> {
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
                components: Some(vec![
                    Component::ActionRow(ActionRow {
                        components: vec![Component::TextInput(TextInput {
                            custom_id: String::from("website-url"),
                            label: String::from("Website URL"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("https://example.com")),
                            required: Some(true),
                            style: TextInputStyle::Short,
                            value: None,
                        })],
                    }),
                    Component::ActionRow(ActionRow {
                        components: vec![Component::TextInput(TextInput {
                            custom_id: String::from("reason"),
                            label: String::from("Reason"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from(
                                "Why should this website be turned into an extension...",
                            )),
                            required: Some(true),
                            style: TextInputStyle::Paragraph,
                            value: None,
                        })],
                    }),
                ]),
                content: None,
                custom_id: Some(String::from("extension-request")),
                embeds: None,
                flags: None,
                title: Some(String::from("Extension Request")),
                tts: None,
            }),
        };

        discord_request(
            &discord_bot::plugin::discord_types::Requests::InteractionCallback((
                interaction_create.id.get(),
                interaction_create.token.clone(),
                simd_json::to_vec(&modal).unwrap(),
            )),
        )?;

        Ok(())
    }

    fn extension_request(interaction_create: &InteractionCreate) -> Result<(), String> {
        let InteractionData::ModalSubmit(modal_interaction_data) =
            interaction_create.data.as_ref().unwrap()
        else {
            unreachable!()
        };

        discord_request(&Requests::InteractionCallback((
            interaction_create.id.get(),
            interaction_create.token.clone(),
            simd_json::to_vec(&InteractionResponse {
                kind: InteractionResponseType::DeferredChannelMessageWithSource,
                data: Some(InteractionResponseData {
                    allowed_mentions: None,
                    attachments: None,
                    choices: None,
                    components: None,
                    content: None,
                    custom_id: None,
                    embeds: None,
                    flags: Some(MessageFlags::EPHEMERAL),
                    title: None,
                    tts: None,
                }),
            })
            .unwrap(),
        )))?;

        let embeds = Embed {
            author: None,
            color: Some(0xE72323),
            description: None,
            fields: vec![],
            footer: Some(EmbedFooter {
                icon_url: interaction_create
                    .member
                    .as_ref()
                    .unwrap()
                    .user
                    .as_ref()
                    .unwrap()
                    .avatar
                    .map(|avatar| {
                        format!(
                            "https://cdn.discordapp.com/avatars/{}/{}.webp",
                            interaction_create
                                .member
                                .as_ref()
                                .unwrap()
                                .user
                                .as_ref()
                                .unwrap()
                                .id
                                .get(),
                            avatar
                        )
                    }),
                proxy_icon_url: None,
                text: format!(
                    "Requested by {}",
                    interaction_create
                        .member
                        .as_ref()
                        .unwrap()
                        .user
                        .as_ref()
                        .unwrap()
                        .name
                        .clone()
                ),
            }),
            image: None,
            kind: String::from("rich"),
            provider: None,
            thumbnail: None,
            timestamp: Some(
                Timestamp::from_secs(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or(Duration::new(0, 0))
                        .as_secs()
                        .try_into()
                        .unwrap_or(0),
                )
                .unwrap_or(Timestamp::from_secs(0).unwrap()),
            ),
            title: None,
            url: None,
            video: None,
        };

        let mut extension_request_embed = embeds.clone();

        let mut response_message = CreateMessage {
            allowed_mentions: Some(AllowedMentions {
                parse: vec![MentionType::Users],
                replied_user: false,
                roles: vec![],
                users: vec![],
            }),
            content: None,
            embeds: Some(vec![embeds]),
            flags: Some(MessageFlags::EPHEMERAL),
        };

        let Ok(url) = Self::url_validation(
            response_message
                .embeds
                .as_mut()
                .unwrap()
                .iter_mut()
                .next()
                .unwrap(),
            modal_interaction_data.components[0].components[0]
                .value
                .as_ref()
                .unwrap(),
        ) else {
            discord_request(
                &discord_bot::plugin::discord_types::Requests::UpdateInteractionOriginal((
                    interaction_create.application_id.get(),
                    interaction_create.token.clone(),
                    simd_json::to_vec(&response_message).unwrap(),
                )),
            )?;
            return Ok(());
        };

        let mut extension_request_title = url.host_str().unwrap().to_string();

        if url.path() != "/" {
            extension_request_title += url.path();
        }

        let mut get_active_threads_response = discord_request(&Requests::GetActiveThreads(
            interaction_create.guild_id.unwrap().get(),
        ))?
        .unwrap();

        let active_threads =
            match simd_json::from_slice::<ThreadsListing>(&mut get_active_threads_response) {
                Ok(active_threads) => active_threads,
                Err(err) => return Err(err.to_string()),
            };

        if let Some(existing_extension_request_thread) =
            active_threads.threads.into_iter().find(|t| {
                t.parent_id.unwrap().get()
                    == CONTEXT
                        .storred_settings
                        .read()
                        .unwrap()
                        .extension_requests_channel_id
                    && t.name.as_ref().unwrap_or(&String::new()) == &extension_request_title
            })
        {
            let embed = response_message
                .embeds
                .as_mut()
                .unwrap()
                .iter_mut()
                .next()
                .unwrap();

            embed.title = Some(String::from("Extension Request Already Exists"));

            embed.description = Some(format!(
                "An extension request for this website already exists: <#{}>",
                &existing_extension_request_thread.id
            ));

            discord_request(
                &discord_bot::plugin::discord_types::Requests::UpdateInteractionOriginal((
                    interaction_create.application_id.get(),
                    interaction_create.token.clone(),
                    simd_json::to_vec(&response_message).unwrap(),
                )),
            )?;

            return Ok(());
        }

        extension_request_embed.title = Some(extension_request_title.clone());
        extension_request_embed.url = Some(url.to_string());
        extension_request_embed.description = Some(format!(
            "**Reason**\n{}",
            modal_interaction_data.components[1].components[0]
                .value
                .as_ref()
                .unwrap()
        ));

        let forum_thead = CreateForumThread {
            name: extension_request_title,
            message: CreateMessage {
                allowed_mentions: Some(AllowedMentions {
                    parse: vec![MentionType::Users],
                    replied_user: false,
                    roles: vec![],
                    users: vec![],
                }),
                content: None,
                embeds: Some(vec![extension_request_embed]),
                flags: Some(MessageFlags::EPHEMERAL),
            },
            applied_tags: CONTEXT
                .storred_settings
                .read()
                .unwrap()
                .tags
                .creation
                .clone(),
        };

        let mut create_forum_thread_response = discord_request(&Requests::CreateForumThread((
            CONTEXT
                .storred_settings
                .read()
                .unwrap()
                .extension_requests_channel_id,
            simd_json::to_vec(&forum_thead).unwrap(),
        )))?
        .unwrap();

        let extension_request_thread =
            match simd_json::from_slice::<Channel>(&mut create_forum_thread_response) {
                Ok(extension_request_thread) => extension_request_thread,
                Err(err) => return Err(err.to_string()),
            };

        let embed = response_message
            .embeds
            .as_mut()
            .unwrap()
            .iter_mut()
            .next()
            .unwrap();

        embed.title = Some(String::from("Created Extension Request"));

        embed.description = Some(format!(
            "An extension request had been created: <#{}>",
            extension_request_thread.id
        ));

        discord_request(&Requests::UpdateInteractionOriginal((
            interaction_create.application_id.get(),
            interaction_create.token.clone(),
            simd_json::to_vec(&response_message).unwrap(),
        )))?;

        Ok(())
    }

    fn url_validation(embed: &mut Embed, url_str: &str) -> Result<Url, ()> {
        let url = match Url::parse(url_str) {
            Ok(url) => url,
            Err(err) => {
                embed.title = Some(String::from("URL Error"));
                embed.description =
                    Some(format!("The provided URL was not valid, error: {}", &err));
                return Err(());
            }
        };

        if url.scheme() != "https" {
            embed.title = Some(String::from("URL Error"));
            embed.description = Some(String::from(
                "The provided URL did not use the https origin.",
            ));
            return Err(());
        }

        Ok(url)
    }
}

export!(Plugin);
