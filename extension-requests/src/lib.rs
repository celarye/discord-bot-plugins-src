use std::{
    sync::{LazyLock, RwLock},
    time::SystemTime,
};

use serde::Deserialize;
use twilight_http::{Client, request::TryIntoRequest};
use twilight_model::{
    application::{
        command::{Command, CommandType},
        interaction::{
            InteractionContextType, InteractionData,
            modal::{
                ModalInteractionComponent, ModalInteractionStringSelect, ModalInteractionTextInput,
            },
        },
    },
    channel::{
        Channel, ChannelType,
        message::{
            AllowedMentions, Component, Embed, EmojiReactionType, MentionType, MessageFlags,
            component::{
                Label, SelectMenu, SelectMenuOption, SelectMenuType, TextInput, TextInputStyle,
            },
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

wit_bindgen::generate!({ path: "../wit" });

use crate::{
    discord_bot::plugin::{
        discord_types::{Contents, Requests},
        host_functions::discord_request,
        plugin_types::{
            RegistrationsRequest, RegistrationsRequestDiscordEvents,
            RegistrationsRequestInteractionCreate, SupportedRegistrations,
        },
    },
    exports::discord_bot::plugin::plugin_functions::{DiscordEvents, Guest},
};

struct Plugin {
    settings: RwLock<PluginSettings>,
}

#[derive(Deserialize)]
struct PluginSettings {
    channel_id: u64,
    tags: PluginSettingsTags,
}

#[derive(Deserialize)]
struct PluginSettingsTags {
    content_service: u64,
    tracker_service: u64,
}

static CONTEXT: LazyLock<Plugin> = LazyLock::new(|| Plugin {
    settings: RwLock::new(PluginSettings {
        channel_id: 0,
        tags: PluginSettingsTags {
            content_service: 0,
            tracker_service: 0,
        },
    }),
});

impl Guest for Plugin {
    fn initialization(
        settings: Vec<u8>,
        supported_registrations: SupportedRegistrations,
    ) -> Result<RegistrationsRequest, String> {
        if !supported_registrations
            .contains(SupportedRegistrations::DISCORD_EVENT_INTERACTION_CREATE)
        {
            return Err(String::from(
                "This plugin requires the interactionCreate event to be enabled.",
            ));
        }

        let settings = match sonic_rs::from_slice::<PluginSettings>(&settings) {
            Ok(settings) => settings,
            Err(err) => {
                return Err(format!(
                    "The provided settings were of the incorrect structure, error: {}",
                    &err,
                ));
            }
        };

        let get_channel_response =
            discord_request(&Requests::GetChannel(settings.channel_id))?.unwrap();

        let channel = match sonic_rs::from_slice::<Channel>(&get_channel_response) {
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

        CONTEXT.settings.write().unwrap().channel_id = settings.channel_id;

        if !channel
            .available_tags
            .as_ref()
            .unwrap()
            .iter()
            .any(|forum_tag| forum_tag.id == settings.tags.content_service)
        {
            return Err(String::from(
                "The provided content service tag ID was not available in the provided forum channel.",
            ));
        }

        if !channel
            .available_tags
            .as_ref()
            .unwrap()
            .iter()
            .any(|forum_tag| forum_tag.id == settings.tags.tracker_service)
        {
            return Err(String::from(
                "The provided tracker service tag ID was not available in the provided forum channel.",
            ));
        }

        CONTEXT.settings.write().unwrap().tags = settings.tags;

        let commands = vec![
            sonic_rs::to_vec(&Command {
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
        ];

        Ok(RegistrationsRequest {
            discord_events: Some(RegistrationsRequestDiscordEvents {
                interaction_create: Some(RegistrationsRequestInteractionCreate {
                    application_commands: Some(commands),
                    message_components: None,
                    modals: Some(vec![String::from("extension-request")]),
                }),
                message_create: false,
                thread_create: false,
                thread_delete: false,
                thread_list_sync: false,
                thread_member_update: false,
                thread_members_update: false,
                thread_update: false,
            }),
            scheduled_jobs: None,
            dependency_functions: None,
        })
    }

    fn shutdown() -> Result<(), String> {
        Ok(())
    }

    fn discord_event(event: DiscordEvents) -> Result<(), String> {
        match event {
            DiscordEvents::InteractionCreate(interaction_create) => {
                let interaction_create =
                    sonic_rs::from_slice::<InteractionCreate>(&interaction_create).unwrap();

                match &interaction_create.data {
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

    fn dependency_function(_function: String, _params: Vec<u8>) -> Result<Vec<u8>, String> {
        unimplemented!();
    }
}

impl Plugin {
    #[allow(clippy::too_many_lines)]
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
                    Component::Label(Label {
                        id: None,
                        label: String::from("Website URL"),
                        description: None,
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("website-url"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("https://example.com")),
                            required: Some(true),
                            style: TextInputStyle::Short,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("Website Type"),
                        description: None,
                        component: Box::new(Component::SelectMenu(SelectMenu {
                            id: None,
                            channel_types: None,
                            custom_id: String::from("website-type"),
                            default_values: None,
                            disabled: false,
                            kind: SelectMenuType::Text,
                            max_values: Some(2),
                            min_values: Some(1),
                            options: Some(vec![
                                SelectMenuOption {
                                    default: true,
                                    description: Some(String::from(
                                        "Websites which provide content services (e.g. MangaDex).",
                                    )),
                                    emoji: Some(EmojiReactionType::Unicode {
                                        name: String::from("📚"),
                                    }),
                                    label: String::from("Content Service"),
                                    value: String::from("content-service"),
                                },
                                SelectMenuOption {
                                    default: false,
                                    description: Some(String::from(
                                        "Websites which provide tracker services (e.g. AniList).",
                                    )),
                                    emoji: Some(EmojiReactionType::Unicode {
                                        name: String::from("🗳️"),
                                    }),
                                    label: String::from("Tracker Service"),
                                    value: String::from("tracker-service"),
                                },
                            ]),
                            placeholder: None,
                            required: Some(true),
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("Reason"),
                        description: None,
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("reason"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from(
                                "Why should this website be turned into an extension...",
                            )),
                            required: Some(true),
                            style: TextInputStyle::Paragraph,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                ]),
                content: None,
                custom_id: Some(String::from("extension-request")),
                embeds: None,
                flags: None,
                title: Some(String::from("Extension Request")),
                tts: None,
                poll: None,
            }),
        };

        discord_request(
            &discord_bot::plugin::discord_types::Requests::InteractionCallback((
                interaction_create.id.get(),
                interaction_create.token.clone(),
                true,
                sonic_rs::to_vec(&modal).unwrap(),
            )),
        )?;

        Ok(())
    }

    fn extension_request(interaction_create: &InteractionCreate) -> Result<(), String> {
        let (
            extension_request_website_url,
            extension_request_website_type,
            extension_request_reason,
        ) = Self::parse_modal(interaction_create);

        discord_request(&Requests::InteractionCallback((
            interaction_create.id.get(),
            interaction_create.token.clone(),
            true,
            sonic_rs::to_vec(&InteractionResponse {
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
                    poll: None,
                }),
            })
            .unwrap(),
        )))?;

        let client = Client::builder().build();

        let Some(url) =
            Self::validate_url(interaction_create, &extension_request_website_url.value)?
        else {
            return Ok(());
        };

        let mut extension_request_title = url.host_str().unwrap().to_string();

        if url.path() != "/" {
            extension_request_title += url.path();
        }

        if Self::forum_thread_existance(&client, interaction_create, &extension_request_title)? {
            return Ok(());
        }

        let extension_request_thread = Self::create_forum_thread(
            interaction_create,
            &extension_request_title,
            &url,
            extension_request_website_type,
            extension_request_reason,
        )?;

        let mut embed = Self::base_embed(interaction_create);

        embed.title = Some(String::from("Created Extension Request"));

        embed.description = Some(format!(
            "An extension request has been created: <#{}>",
            extension_request_thread.id
        ));

        let response_message_request = match client
            .create_message(interaction_create.channel.as_ref().unwrap().id)
            .flags(MessageFlags::EPHEMERAL)
            .embeds(&[embed])
            .try_into_request()
        {
            Ok(response_message_request) => response_message_request,
            Err(err) => {
                return Err(format!(
                    "An error occured while building the response message request: {err}"
                ));
            }
        };

        discord_request(
            &discord_bot::plugin::discord_types::Requests::UpdateInteractionOriginal((
                interaction_create.application_id.get(),
                interaction_create.token.clone(),
                response_message_request.body().unwrap().to_owned(),
            )),
        )?;

        Ok(())
    }

    fn parse_modal(
        interaction_create: &InteractionCreate,
    ) -> (
        &ModalInteractionTextInput,
        &ModalInteractionStringSelect,
        &ModalInteractionTextInput,
    ) {
        let InteractionData::ModalSubmit(modal_interaction_data) =
            interaction_create.data.as_ref().unwrap()
        else {
            unreachable!()
        };

        let ModalInteractionComponent::TextInput(extension_request_website_url) = ({
            let ModalInteractionComponent::Label(extension_request_website_url_label) =
                &modal_interaction_data.components[0]
            else {
                unreachable!()
            };

            extension_request_website_url_label.component.as_ref()
        }) else {
            unreachable!()
        };

        let ModalInteractionComponent::StringSelect(extension_request_website_type) = ({
            let ModalInteractionComponent::Label(extension_request_website_type_label) =
                &modal_interaction_data.components[1]
            else {
                unreachable!()
            };

            extension_request_website_type_label.component.as_ref()
        }) else {
            unreachable!()
        };

        let ModalInteractionComponent::TextInput(extension_request_reason) = ({
            let ModalInteractionComponent::Label(extension_request_reason_label) =
                &modal_interaction_data.components[2]
            else {
                unreachable!()
            };

            extension_request_reason_label.component.as_ref()
        }) else {
            unreachable!()
        };

        (
            extension_request_website_url,
            extension_request_website_type,
            extension_request_reason,
        )
    }

    fn base_embed(interaction_create: &InteractionCreate) -> Embed {
        Embed {
            author: None,
            color: Some(0x00E7_2323),
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
                        .elapsed()
                        .unwrap_or_default()
                        .as_secs()
                        .try_into()
                        .unwrap_or_default(),
                )
                .unwrap_or(Timestamp::from_secs(0).unwrap()),
            ),
            title: None,
            url: None,
            video: None,
        }
    }

    fn forum_thread_existance(
        client: &Client,
        interaction_create: &InteractionCreate,
        extension_request_title: &str,
    ) -> Result<bool, String> {
        let get_active_threads_response = discord_request(&Requests::GetActiveThreads(
            interaction_create.guild_id.unwrap().get(),
        ))?
        .unwrap();

        let active_threads =
            match sonic_rs::from_slice::<ThreadsListing>(&get_active_threads_response) {
                Ok(active_threads) => active_threads,
                Err(err) => return Err(err.to_string()),
            };

        if let Some(existing_extension_request_thread) =
            active_threads.threads.into_iter().find(|t| {
                t.parent_id.unwrap().get() == CONTEXT.settings.read().unwrap().channel_id
                    && t.name.as_ref().unwrap_or(&String::new()) == extension_request_title
            })
        {
            let mut embed = Self::base_embed(interaction_create);

            embed.title = Some(String::from("Extension Request Already Exists"));

            embed.description = Some(format!(
                "An extension request for this website already exists: <#{}>",
                &existing_extension_request_thread.id
            ));

            let response_message_request = match client
                .create_message(interaction_create.channel.as_ref().unwrap().id)
                .flags(MessageFlags::EPHEMERAL)
                .embeds(&[embed])
                .try_into_request()
            {
                Ok(response_message_request) => response_message_request,
                Err(err) => {
                    return Err(format!(
                        "An error occured while building the response message request: {err}"
                    ));
                }
            };

            discord_request(
                &discord_bot::plugin::discord_types::Requests::UpdateInteractionOriginal((
                    interaction_create.application_id.get(),
                    interaction_create.token.clone(),
                    response_message_request.body().unwrap().to_owned(),
                )),
            )?;

            return Ok(true);
        }

        Ok(false)
    }

    fn create_forum_thread(
        interaction_create: &InteractionCreate,
        extension_request_title: &str,
        url: &Url,
        extension_request_website_type: &ModalInteractionStringSelect,
        extension_request_reason: &ModalInteractionTextInput,
    ) -> Result<Channel, String> {
        let mut embed = Self::base_embed(interaction_create);

        embed.title = Some(extension_request_title.to_string());
        embed.url = Some(url.to_string());

        let mut extension_request_tags = vec![];

        for website_type in &extension_request_website_type.values {
            match website_type.as_str() {
                "content-service" => extension_request_tags.push(Id::new(
                    CONTEXT.settings.read().unwrap().tags.content_service,
                )),
                "tracker-service" => extension_request_tags.push(Id::new(
                    CONTEXT.settings.read().unwrap().tags.tracker_service,
                )),
                &_ => unreachable!(),
            }
        }

        embed.description = Some(format!("**Reason**\n{}", &extension_request_reason.value));

        let client = Client::builder().build();

        let forum_thread_request = match client
            .create_forum_thread(
                Id::new(CONTEXT.settings.read().unwrap().channel_id),
                extension_request_title,
            )
            .applied_tags(&extension_request_tags)
            .message()
            .embeds(&[embed])
            .try_into_request()
        {
            Ok(forum_thread_request) => forum_thread_request,
            Err(err) => {
                return Err(format!(
                    "An error occured while making a forum thread request: {err}"
                ));
            }
        };

        let create_forum_thread_response = discord_request(&Requests::CreateForumThread((
            CONTEXT.settings.read().unwrap().channel_id,
            Contents::Json(forum_thread_request.body().unwrap().to_owned()),
        )))?
        .unwrap();

        match sonic_rs::from_slice::<Channel>(&create_forum_thread_response) {
            Ok(extension_request_thread) => Ok(extension_request_thread),
            Err(err) => Err(err.to_string()),
        }
    }

    fn validate_url(
        interaction_create: &InteractionCreate,
        url_str: &str,
    ) -> Result<Option<Url>, String> {
        let embed = match Url::parse(url_str) {
            Ok(url) => {
                if url.scheme() == "https" {
                    return Ok(Some(url));
                }

                let mut embed = Self::base_embed(interaction_create);

                embed.title = Some(String::from("URL Error"));
                embed.description = Some(String::from(
                    "The provided URL did not use the HTTPS origin. URLs should always start with \"https://\".",
                ));

                embed
            }
            Err(err) => {
                let mut embed = Self::base_embed(interaction_create);

                embed.title = Some(String::from("URL Error"));
                embed.description =
                    Some(format!("The provided URL was not valid, error: {}", &err));

                embed
            }
        };

        let client = Client::builder().build();

        let response_message_request = match client
            .create_message(interaction_create.channel.as_ref().unwrap().id)
            .flags(MessageFlags::EPHEMERAL)
            .embeds(&[embed])
            .try_into_request()
        {
            Ok(response_message_request) => response_message_request,
            Err(err) => {
                return Err(format!(
                    "An error occured while building the response message request: {err}"
                ));
            }
        };

        discord_request(
            &discord_bot::plugin::discord_types::Requests::UpdateInteractionOriginal((
                interaction_create.application_id.get(),
                interaction_create.token.clone(),
                response_message_request.body().unwrap().to_owned(),
            )),
        )?;

        Ok(None)
    }
}

export!(Plugin);
