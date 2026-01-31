use std::{
    sync::{Arc, LazyLock, RwLock},
    time::SystemTime,
};

use serde::Deserialize;
use twilight_http::{Client, request::TryIntoRequest};
use twilight_model::{
    channel::{
        Channel,
        message::{Embed, embed::EmbedFooter},
    },
    gateway::payload::incoming::MessageCreate,
    guild::PartialMember,
    id::Id,
    util::Timestamp,
};
use wstd::runtime::{block_on, spawn};

wit_bindgen::generate!({ path: "../wit" });

use crate::{
    discord_bot::plugin::{
        discord_types::{Contents, Requests},
        host_functions::discord_request,
        plugin_types::{
            RegistrationsRequest, RegistrationsRequestDiscordEvents, SupportedRegistrations,
        },
    },
    exports::discord_bot::plugin::plugin_functions::{DiscordEvents, Guest},
};

struct Plugin {
    settings: RwLock<PluginSettings>,
}

#[derive(Deserialize)]
struct PluginSettings {
    #[serde(default)]
    validations: PluginSettingsValidations,
    automod_channel_id: u64,
    bypass: Option<PluginSettingsBypass>,
}

#[derive(Deserialize)]
struct PluginSettingsValidations {
    attachment_spam: Option<PluginSettingsAttachmentSpam>,
}

impl Default for PluginSettingsValidations {
    fn default() -> Self {
        PluginSettingsValidations {
            attachment_spam: Some(PluginSettingsAttachmentSpam {
                count: 3,
                actions: vec![
                    Actions::Report,
                    Actions::DeleteMessage,
                    Actions::TimeOutMember(60),
                ],
            }),
        }
    }
}

#[derive(Deserialize)]
struct PluginSettingsAttachmentSpam {
    count: usize,
    actions: Vec<Actions>,
}

#[derive(Deserialize)]
struct PluginSettingsBypass {
    #[serde(default)]
    users: Vec<u64>,
    #[serde(default)]
    roles: Vec<u64>,
}

#[derive(Deserialize)]
enum Actions {
    Report,
    DeleteMessage,
    BanMember,
    TimeOutMember(u64),
}

static CONTEXT: LazyLock<Plugin> = LazyLock::new(|| Plugin {
    settings: RwLock::new(PluginSettings {
        validations: PluginSettingsValidations::default(),
        automod_channel_id: 0,
        bypass: None,
    }),
});

impl Guest for Plugin {
    fn initialization(
        settings: Vec<u8>,
        supported_registrations: SupportedRegistrations,
    ) -> Result<RegistrationsRequest, String> {
        if !supported_registrations.contains(SupportedRegistrations::DISCORD_EVENT_MESSAGE_CREATE) {
            return Err(String::from(
                "This plugin requires the messageCreate event to be enabled.",
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

        let get_channel_response = match discord_request(&Requests::GetChannel(
            settings.automod_channel_id,
        )) {
            Ok(get_channel_response) => get_channel_response,
            Err(err) => {
                return Err(format!(
                    "An error occured while trying to get information of the automod channel: {err}"
                ));
            }
        };

        if let Err(err) = sonic_rs::from_slice::<Channel>(get_channel_response.as_ref().unwrap()) {
            return Err(format!(
                "An error occured while deserializing the get channel response from Discord: {}",
                &err
            ));
        };

        CONTEXT.settings.write().unwrap().validations = settings.validations;

        CONTEXT.settings.write().unwrap().automod_channel_id = settings.automod_channel_id;

        CONTEXT.settings.write().unwrap().bypass = settings.bypass;

        Ok(RegistrationsRequest {
            discord_events: Some(RegistrationsRequestDiscordEvents {
                interaction_create: None,
                message_create: true,
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

    fn shutdown() -> Result<(), _rt::String> {
        Ok(())
    }

    fn discord_event(event: DiscordEvents) -> Result<(), String> {
        match event {
            DiscordEvents::MessageCreate(message_bytes) => {
                match sonic_rs::from_slice::<Box<MessageCreate>>(&message_bytes) {
                    Ok(message) => Self::validate_message(Arc::new(*message)),
                    Err(err) => Err(err.to_string()),
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
    fn validate_message(message: Arc<MessageCreate>) -> Result<(), String> {
        if Self::bypass(message.member.as_ref().unwrap()) {
            return Ok(());
        }

        let mut tasks = vec![];

        block_on(async {
            let message_v1 = message.clone();

            tasks.push(spawn(async {
                if let Some(attachment_spam) = CONTEXT
                    .settings
                    .read()
                    .unwrap()
                    .validations
                    .attachment_spam
                    .as_ref()
                {
                    return Self::attachment_spam(attachment_spam, message_v1);
                }

                Ok(())
            }));

            for task in tasks.drain(..) {
                task.await?
            }

            Ok(())
        })
    }

    fn bypass(member: &PartialMember) -> bool {
        if let Some(bypass) = &CONTEXT.settings.read().unwrap().bypass {
            if bypass
                .users
                .contains(&member.user.as_ref().unwrap().id.get())
            {
                return true;
            }

            for member_role in &member.roles {
                if bypass.roles.contains(&member_role.get()) {
                    return true;
                }
            }
        }

        false
    }

    fn attachment_spam(
        attachment_spam: &PluginSettingsAttachmentSpam,
        message: Arc<MessageCreate>,
    ) -> Result<(), String> {
        let attachment_count = message.attachments.len();

        if attachment_count >= attachment_spam.count {
            let reason = format!(
                "Attachment spam - {} attachments, without message content",
                attachment_count
            );

            return Self::take_action(&attachment_spam.actions, &reason, message);
        }

        Ok(())
    }

    fn take_action(
        actions: &[Actions],
        reason: &str,
        message: Arc<MessageCreate>,
    ) -> Result<(), String> {
        let client = Client::builder().build();

        for action in actions {
            match action {
                Actions::Report => Self::report(&client, actions, reason, &message)?,
                Actions::DeleteMessage => Self::delete_message(&message)?,
                Actions::TimeOutMember(period) => {
                    Self::time_out_member(&client, &message, *period)?
                }
                Actions::BanMember => Self::ban_member(&message)?,
            }
        }

        Ok(())
    }

    fn report(
        client: &Client,
        actions: &[Actions],
        reason: &str,
        message: &Arc<MessageCreate>,
    ) -> Result<(), String> {
        let mut embed = Self::base_embed(message);

        embed.title = Some(format!(
            "Member {} triggered automod!",
            message.member.as_ref().unwrap().user.as_ref().unwrap().name
        ));

        embed.description = Some(format!("**Reason:** {}\n\n**Actions Taken:\n**", reason));

        if actions.len() > 1 {
            for action in actions {
                match action {
                    Actions::Report => (),
                    Actions::DeleteMessage => embed
                        .description
                        .as_mut()
                        .unwrap()
                        .push_str("- Message deleted"),
                    Actions::BanMember => embed.description.as_mut().unwrap().push_str("- Banned"),
                    Actions::TimeOutMember(period) => embed
                        .description
                        .as_mut()
                        .unwrap()
                        .push_str(&format!("- Timed out for {} seconds", period)),
                }
            }
        } else {
            embed.description.as_mut().unwrap().push_str("None");
        }

        let create_message_request = match client
            .create_message(Id::new(CONTEXT.settings.read().unwrap().automod_channel_id))
            .embeds(&[embed])
            .try_into_request()
        {
            Ok(create_message_request) => create_message_request,
            Err(err) => {
                return Err(format!(
                    "An error occured while creating the report create message request: {}",
                    err
                ));
            }
        };

        discord_request(&Requests::CreateMessage((
            CONTEXT.settings.read().unwrap().automod_channel_id,
            Contents::Json(create_message_request.body().unwrap().to_owned()),
        )))?;

        Ok(())
    }

    fn delete_message(message: &Arc<MessageCreate>) -> Result<(), String> {
        discord_request(&Requests::DeleteMessage((
            message.channel_id.get(),
            message.id.get(),
        )))?;

        Ok(())
    }

    fn time_out_member(
        client: &Client,
        message: &Arc<MessageCreate>,
        period: u64,
    ) -> Result<(), String> {
        let update_guild_member_request = match client
            .update_guild_member(
                message.guild_id.unwrap(),
                message.member.as_ref().unwrap().user.as_ref().unwrap().id,
            )
            .communication_disabled_until(Some(
                Timestamp::from_secs(
                    (SystemTime::now().elapsed().unwrap_or_default().as_secs() + period)
                        .try_into()
                        .unwrap_or_default(),
                )
                .unwrap_or(Timestamp::from_secs(0).unwrap()),
            ))
            .try_into_request()
        {
            Ok(update_guild_member_request) => update_guild_member_request,
            Err(err) => {
                return Err(format!(
                    "An error occured while creating the report message request: {}",
                    err
                ));
            }
        };

        discord_request(&Requests::UpdateMember((
            message.guild_id.unwrap().get(),
            message
                .member
                .as_ref()
                .unwrap()
                .user
                .as_ref()
                .unwrap()
                .id
                .get(),
            update_guild_member_request.body().unwrap().to_owned(),
        )))?;

        Ok(())
    }

    fn ban_member(message: &Arc<MessageCreate>) -> Result<(), String> {
        discord_request(&Requests::DeleteMessage((
            message.channel_id.get(),
            message.id.get(),
        )))?;

        Ok(())
    }

    fn base_embed(message: &Arc<MessageCreate>) -> Embed {
        Embed {
            author: None,
            color: Some(0x00E7_2323),
            description: None,
            fields: vec![],
            footer: Some(EmbedFooter {
                icon_url: message
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
                            message
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
                text: message
                    .member
                    .as_ref()
                    .unwrap()
                    .user
                    .as_ref()
                    .unwrap()
                    .name
                    .clone(),
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
}

export!(Plugin);
