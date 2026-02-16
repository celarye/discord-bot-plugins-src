use std::{
    sync::{LazyLock, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use twilight_http::{Client as DiscordClient, request::TryIntoRequest};
use twilight_model::{
    application::{
        command::{Command, CommandType},
        interaction::{
            InteractionContextType, InteractionData,
            modal::{
                ModalInteractionComponent, ModalInteractionData, ModalInteractionFileUpload,
                ModalInteractionTextInput,
            },
        },
    },
    channel::{
        Channel, ChannelType,
        message::{
            AllowedMentions, Component, Embed, MentionType, MessageFlags,
            component::{FileUpload, Label, TextInput, TextInputStyle},
            embed::EmbedFooter,
        },
    },
    gateway::payload::incoming::InteractionCreate,
    guild::Permissions,
    http::{
        attachment::Attachment,
        interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    },
    id::{Id, marker::AttachmentMarker},
    oauth::ApplicationIntegrationType,
    util::Timestamp,
};
use wstd::{
    http::{Client, Request},
    runtime::block_on,
};

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
    question: u64,
    bug: u64,
    enhancement: u64,
    needs_triage: u64,
}

static CONTEXT: LazyLock<Plugin> = LazyLock::new(|| Plugin {
    settings: RwLock::new(PluginSettings {
        channel_id: 0,
        tags: PluginSettingsTags {
            question: 0,
            bug: 0,
            enhancement: 0,
            needs_triage: 0,
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
                "The provided support channel needs to be of the forum type.",
            ));
        }

        CONTEXT.settings.write().unwrap().channel_id = settings.channel_id;

        if !channel
            .available_tags
            .as_ref()
            .unwrap()
            .iter()
            .any(|forum_tag| forum_tag.id == settings.tags.question)
        {
            return Err(String::from(
                "The provided question tag ID was not available in the provided forum channel.",
            ));
        }

        if !channel
            .available_tags
            .as_ref()
            .unwrap()
            .iter()
            .any(|forum_tag| forum_tag.id == settings.tags.bug)
        {
            return Err(String::from(
                "The provided bug tag ID was not available in the provided forum channel.",
            ));
        }

        if !channel
            .available_tags
            .as_ref()
            .unwrap()
            .iter()
            .any(|forum_tag| forum_tag.id == settings.tags.enhancement)
        {
            return Err(String::from(
                "The provided enhancement tag ID was not available in the provided forum channel.",
            ));
        }

        if !channel
            .available_tags
            .as_ref()
            .unwrap()
            .iter()
            .any(|forum_tag| forum_tag.id == settings.tags.needs_triage)
        {
            return Err(String::from(
                "The provided needs trage tag ID was not available in the provided forum channel.",
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
                description: String::from("Ask questions not related to bugs or enhancements."),
                description_localizations: None,
                guild_id: Some(channel.guild_id.unwrap()),
                id: None,
                integration_types: Some(vec![ApplicationIntegrationType::GuildInstall]),
                kind: CommandType::ChatInput,
                name: String::from("support-question"),
                name_localizations: None,
                nsfw: Some(false),
                options: vec![],
                version: Id::new(1),
            })
            .unwrap(),
            sonic_rs::to_vec(&Command {
                application_id: None,
                contexts: Some(vec![InteractionContextType::Guild]),
                default_member_permissions: Some(Permissions::SEND_MESSAGES),
                #[allow(deprecated)]
                dm_permission: None,
                description: String::from("Report bugs with extensions, websites or tooling."),
                description_localizations: None,
                guild_id: Some(channel.guild_id.unwrap()),
                id: None,
                integration_types: Some(vec![ApplicationIntegrationType::GuildInstall]),
                kind: CommandType::ChatInput,
                name: String::from("support-bug"),
                name_localizations: None,
                nsfw: Some(false),
                options: vec![],
                version: Id::new(1),
            })
            .unwrap(),
            sonic_rs::to_vec(&Command {
                application_id: None,
                contexts: Some(vec![InteractionContextType::Guild]),
                default_member_permissions: Some(Permissions::SEND_MESSAGES),
                #[allow(deprecated)]
                dm_permission: None,
                description: String::from(
                    "Request an enhancement to our extensions, website or tooling.",
                ),
                description_localizations: None,
                guild_id: Some(channel.guild_id.unwrap()),
                id: None,
                integration_types: Some(vec![ApplicationIntegrationType::GuildInstall]),
                kind: CommandType::ChatInput,
                name: String::from("support-enhancement"),
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
                    message_components: Some(vec![
                        String::from("get-support-question"),
                        String::from("get-support-bug"),
                        String::from("get-support-enhancement"),
                    ]),
                    modals: Some(vec![
                        String::from("support-question"),
                        String::from("support-bug"),
                        String::from("support-enhancement"),
                    ]),
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

                match interaction_create.data.as_ref() {
                    Some(InteractionData::ApplicationCommand(command_data)) => {
                        match command_data.name.as_str() {
                            "support-question" => Plugin::get_support_question(&interaction_create),
                            "support-bug" => Plugin::get_support_bug(&interaction_create),
                            "support-enhancement" => {
                                Plugin::get_support_enhancement(&interaction_create)
                            }
                            &_ => unimplemented!(),
                        }
                    }
                    Some(InteractionData::ModalSubmit(modal_interaction_data)) => {
                        match modal_interaction_data.custom_id.as_str() {
                            "support-question" => Plugin::support_question(&interaction_create),
                            "support-bug" => Plugin::support_bug(&interaction_create),
                            "support-enhancement" => {
                                Plugin::support_enhancement(&interaction_create)
                            }
                            &_ => unimplemented!(),
                        }
                    }
                    Some(InteractionData::MessageComponent(message_component_interaction_data)) => {
                        match message_component_interaction_data.custom_id.as_str() {
                            "get-support-question" => {
                                Plugin::get_support_question(&interaction_create)
                            }
                            "get-support-bug" => Plugin::get_support_bug(&interaction_create),
                            "get-support-enhancement" => {
                                Plugin::get_support_enhancement(&interaction_create)
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

    fn dependency_function(_function: String, _params: Vec<u8>) -> Result<Vec<u8>, String> {
        unimplemented!();
    }
}

impl Plugin {
    fn get_support_question(interaction_create: &InteractionCreate) -> Result<(), String> {
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
                        label: String::from("Title"),
                        description: Some(String::from("Be descriptive.")),
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("title"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("What... How... Why...")),
                            required: Some(true),
                            style: TextInputStyle::Short,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("Description"),
                        description: Some(String::from(
                            "Make sure to provide all relevant information.",
                        )),
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("description"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("Tell us about it!")),
                            required: Some(true),
                            style: TextInputStyle::Paragraph,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("File Upload"),
                        description: Some(String::from(
                            "Got images or other files? You can share it with us here.",
                        )),
                        component: Box::new(Component::FileUpload(FileUpload {
                            id: None,
                            custom_id: String::from("files"),
                            max_values: None,
                            min_values: None,
                            required: Some(false),
                        })),
                    }),
                ]),
                content: None,
                custom_id: Some(String::from("support-question")),
                embeds: None,
                flags: None,
                title: Some(String::from("Support Question")),
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

    fn get_support_bug(interaction_create: &InteractionCreate) -> Result<(), String> {
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
                        label: String::from("Name"),
                        description: Some(String::from(
                            "The name of the extension, website or tool.",
                        )),
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("name"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("e.g. MangaDex")),
                            required: Some(true),
                            style: TextInputStyle::Short,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("Version"),
                        description: Some(String::from(
                            "Specify the version you are reporting this bug for.",
                        )),
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("version"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("e.g. v1.0.0-alpha.5")),
                            required: Some(true),
                            style: TextInputStyle::Short,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("URL"),
                        description: Some(String::from("Provide a link to the relevant website.")),
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("url"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("e.g. https://mangadex.org/")),
                            required: Some(true),
                            style: TextInputStyle::Short,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("Description"),
                        description: Some(String::from(
                            "Make sure to provide all relevant information.",
                        )),
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("description"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("Tell us about the bug!")),
                            required: Some(true),
                            style: TextInputStyle::Paragraph,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("File Upload"),
                        description: Some(String::from(
                            "Got images or other files? You can share it with us here.",
                        )),
                        component: Box::new(Component::FileUpload(FileUpload {
                            id: None,
                            custom_id: String::from("files"),
                            max_values: None,
                            min_values: None,
                            required: Some(false),
                        })),
                    }),
                ]),
                content: None,
                custom_id: Some(String::from("support-bug")),
                embeds: None,
                flags: None,
                title: Some(String::from("Support Bug")),
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

    fn get_support_enhancement(interaction_create: &InteractionCreate) -> Result<(), String> {
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
                        label: String::from("Name"),
                        description: Some(String::from(
                            "The name of the extension, website, or tool.",
                        )),
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("name"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("e.g. MangaDex")),
                            required: Some(true),
                            style: TextInputStyle::Short,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("URL"),
                        description: Some(String::from("Provide a link to the relevant website.")),
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("url"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from("e.g. https://mangadex.org/")),
                            required: Some(true),
                            style: TextInputStyle::Short,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("Description"),
                        description: Some(String::from(
                            "Describe the improvement or feature you would like to see and what benefits it will bring!",
                        )),
                        component: Box::new(Component::TextInput(TextInput {
                            id: None,
                            custom_id: String::from("description"),
                            max_length: None,
                            min_length: None,
                            placeholder: Some(String::from(
                                "What should be improved or added? Why would it be useful?",
                            )),
                            required: Some(true),
                            style: TextInputStyle::Paragraph,
                            value: None,
                            #[allow(deprecated)]
                            label: None,
                        })),
                    }),
                    Component::Label(Label {
                        id: None,
                        label: String::from("File Upload"),
                        description: Some(String::from(
                            "Got images or other files? You can share it with us here.",
                        )),
                        component: Box::new(Component::FileUpload(FileUpload {
                            id: None,
                            custom_id: String::from("files"),
                            max_values: None,
                            min_values: None,
                            required: Some(false),
                        })),
                    }),
                ]),
                content: None,
                custom_id: Some(String::from("support-enhancement")),
                embeds: None,
                flags: None,
                title: Some(String::from("Support Enhancement")),
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

    fn support_question(interaction_create: &InteractionCreate) -> Result<(), String> {
        let (modal_interaction_data, title, description, files) =
            Self::parse_support_question_modal(interaction_create);

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

        let support_question_thread = Self::create_support_question_forum_thread(
            interaction_create,
            modal_interaction_data,
            &title.value,
            &description.value,
            &files.values,
        )?;

        let mut embed = Self::base_embed(interaction_create);

        embed.title = Some(String::from("Created Support Question"));

        embed.description = Some(format!(
            "A support question has been created: <#{}>",
            support_question_thread.id
        ));

        let client = DiscordClient::builder().build();

        let request = match client
            .create_message(interaction_create.channel.as_ref().unwrap().id)
            .allowed_mentions(Some(&AllowedMentions {
                parse: vec![MentionType::Users],
                replied_user: false,
                roles: vec![],
                users: vec![],
            }))
            .embeds(&[embed])
            .flags(MessageFlags::EPHEMERAL)
            .try_into_request()
        {
            Ok(request) => request,
            Err(err) => {
                return Err(format!("{err}"));
            }
        };

        discord_request(&Requests::UpdateInteractionOriginal((
            interaction_create.application_id.get(),
            interaction_create.token.clone(),
            request.body().unwrap().to_vec(),
        )))?;

        Ok(())
    }

    fn support_bug(_interaction_create: &InteractionCreate) -> Result<(), String> {
        todo!()
    }

    fn support_enhancement(_interaction_create: &InteractionCreate) -> Result<(), String> {
        todo!()
    }

    fn parse_support_question_modal(
        interaction_create: &InteractionCreate,
    ) -> (
        &ModalInteractionData,
        &ModalInteractionTextInput,
        &ModalInteractionTextInput,
        &ModalInteractionFileUpload,
    ) {
        let InteractionData::ModalSubmit(modal_interaction_data) =
            interaction_create.data.as_ref().unwrap()
        else {
            unreachable!()
        };

        let ModalInteractionComponent::TextInput(support_question_title) = ({
            let ModalInteractionComponent::Label(support_question_title_label) =
                &modal_interaction_data.components[0]
            else {
                unreachable!()
            };

            support_question_title_label.component.as_ref()
        }) else {
            unreachable!()
        };

        let ModalInteractionComponent::TextInput(support_question_description) = ({
            let ModalInteractionComponent::Label(support_question_description_label) =
                &modal_interaction_data.components[1]
            else {
                unreachable!()
            };

            support_question_description_label.component.as_ref()
        }) else {
            unreachable!()
        };

        let ModalInteractionComponent::FileUpload(support_question_files) = ({
            let ModalInteractionComponent::Label(support_question_files_label) =
                &modal_interaction_data.components[2]
            else {
                unreachable!()
            };

            support_question_files_label.component.as_ref()
        }) else {
            unreachable!()
        };

        (
            modal_interaction_data,
            support_question_title,
            support_question_description,
            support_question_files,
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
        }
    }

    fn create_support_question_forum_thread(
        interaction_create: &InteractionCreate,
        modal_interaction_data: &ModalInteractionData,
        title: &str,
        description: &str,
        file_ids: &[Id<AttachmentMarker>],
    ) -> Result<Channel, String> {
        let mut attachments = vec![];

        let client = DiscordClient::builder().build();

        for file_id in file_ids {
            let file = modal_interaction_data
                .resolved
                .as_ref()
                .unwrap()
                .attachments
                .get(file_id)
                .unwrap();

            let http_client = Client::new();

            let request = match Request::get(&file.url).body(()) {
                Ok(request) => request,
                Err(err) => {
                    return Err(format!(
                        "An error occured while building the request to fetch an uploaded file: {err}"
                    ));
                }
            };

            let mut response = match block_on(async { http_client.send(request).await }) {
                Ok(response) => response,
                Err(err) => {
                    return Err(format!(
                        "An error occured while fetching an uploaded file: {err}"
                    ));
                }
            };

            let file_bytes = match block_on(response.body_mut().contents()) {
                Ok(file) => file.to_vec(),
                Err(err) => {
                    return Err(format!(
                        "An error occured while reading the contents of the response body: {err}"
                    ));
                }
            };

            attachments.push(Attachment {
                description: file.description.clone(),
                file: file_bytes,
                filename: file.filename.clone(),
                id: 0,
            });
        }

        let request = match client
            .create_forum_thread(Id::new(CONTEXT.settings.read().unwrap().channel_id), title)
            .applied_tags(&[Id::new(CONTEXT.settings.read().unwrap().tags.question)])
            .message()
            .allowed_mentions(Some(&AllowedMentions {
                parse: vec![MentionType::Users],
                replied_user: false,
                roles: Vec::new(),
                users: Vec::new(),
            }))
            .content(&format!(
                "{}\n\n**Posted by:** <@{}>",
                description,
                interaction_create
                    .member
                    .as_ref()
                    .unwrap()
                    .user
                    .as_ref()
                    .unwrap()
                    .id
                    .get()
            ))
            .attachments(&attachments)
            .try_into_request()
        {
            Ok(request) => request,
            Err(err) => {
                return Err(format!(
                    "An error occured while building the Discord request: {err}"
                ));
            }
        };

        let content = if file_ids.is_empty() {
            Contents::Form(request.body().unwrap().to_owned())
        } else {
            Contents::Form(request.form().unwrap().to_owned().build())
        };

        let create_forum_thread_response = discord_request(&Requests::CreateForumThread((
            CONTEXT.settings.read().unwrap().channel_id,
            content,
        )))?
        .unwrap();

        match sonic_rs::from_slice::<Channel>(&create_forum_thread_response) {
            Ok(support_question_thread) => Ok(support_question_thread),
            Err(err) => Err(err.to_string()),
        }
    }
}

export!(Plugin);
