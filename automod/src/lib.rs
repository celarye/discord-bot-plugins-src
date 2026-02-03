use std::{
    fmt::Write,
    sync::{LazyLock, RwLock},
    time::UNIX_EPOCH,
};

use serde::Deserialize;
use twilight_http::{
    Client,
    request::{AuditLogReason, TryIntoRequest},
};
use twilight_model::{
    channel::{
        Channel,
        message::{
            Embed,
            embed::{EmbedAuthor, EmbedFooter},
        },
    },
    gateway::payload::incoming::MessageCreate,
    id::Id,
    util::Timestamp,
};

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
    automod_channel_id: u64,
    #[serde(default = "PluginSettings::stack_time_outs_default")]
    stack_time_outs: bool,
    bypass: Option<PluginSettingsBypass>,
    #[serde(default)]
    validations: PluginSettingsValidations,
}

impl PluginSettings {
    fn stack_time_outs_default() -> bool {
        true
    }
}

#[derive(Deserialize)]
struct PluginSettingsBypass {
    #[serde(default)]
    users: Vec<u64>,
    #[serde(default)]
    roles: Vec<u64>,
}

#[derive(Deserialize)]
struct PluginSettingsValidations {
    attachment_spam: Option<PluginSettingsAttachmentSpam>,
}

impl Default for PluginSettingsValidations {
    fn default() -> Self {
        Self {
            attachment_spam: Some(PluginSettingsAttachmentSpam::default()),
        }
    }
}

#[derive(Deserialize)]
struct PluginSettingsAttachmentSpam {
    #[serde(default)]
    count: usize,
    #[serde(default)]
    actions: Actions,
}

impl Default for PluginSettingsAttachmentSpam {
    fn default() -> Self {
        Self {
            count: Self::count_default(),
            actions: Actions::default(),
        }
    }
}

impl PluginSettingsAttachmentSpam {
    fn count_default() -> usize {
        4
    }
}

#[derive(Deserialize)]
struct Actions {
    #[serde(default = "Actions::report_default")]
    report: bool,
    #[serde(default)]
    message: Option<ActionsMessage>,
    #[serde(default)]
    user: Option<ActionsUser>,
}

impl Default for Actions {
    fn default() -> Self {
        Self {
            report: Self::report_default(),
            message: Some(Self::message_default()),
            user: Some(Self::user_default()),
        }
    }
}

impl Actions {
    fn report_default() -> bool {
        true
    }

    fn message_default() -> ActionsMessage {
        ActionsMessage::default()
    }

    fn user_default() -> ActionsUser {
        ActionsUser::default()
    }
}

#[derive(Clone, Copy, Default, Deserialize)]
enum ActionsMessage {
    #[default]
    Delete,
}

#[derive(Clone, Copy, Deserialize)]
enum ActionsUser {
    Ban,
    #[serde(rename = "time_out")]
    TimeOut(u64),
}

impl Default for ActionsUser {
    fn default() -> Self {
        Self::TimeOut(60)
    }
}

struct TakeAction {
    report: Option<String>,
    message: Option<ActionsMessage>,
    user: Option<ActionsUser>,
}

static CONTEXT: LazyLock<Plugin> = LazyLock::new(|| Plugin {
    settings: RwLock::new(PluginSettings {
        automod_channel_id: 0,
        stack_time_outs: PluginSettings::stack_time_outs_default(),
        bypass: None,
        validations: PluginSettingsValidations::default(),
    }),
});

impl Guest for Plugin {
    fn initialization(
        settings: Vec<u8>,
        supported_registrations: SupportedRegistrations,
    ) -> Result<RegistrationsRequest, String> {
        if !supported_registrations.contains(SupportedRegistrations::DISCORD_EVENT_MESSAGE_CREATE) {
            return Err(String::from(
                "This plugin requires the MESSAGE_CREATE event to be enabled.",
            ));
        }

        let settings = match sonic_rs::from_slice::<PluginSettings>(&settings) {
            Ok(settings) => settings,
            Err(err) => {
                return Err(format!(
                    "The provided settings were of the incorrect structure: {err}"
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
                "An error occured while deserializing the get channel response from Discord: {err}",
            ));
        }

        let mut ctx_settings = CONTEXT.settings.write().unwrap();

        ctx_settings.automod_channel_id = settings.automod_channel_id;

        ctx_settings.stack_time_outs = settings.stack_time_outs;

        ctx_settings.bypass = settings.bypass;

        ctx_settings.validations = settings.validations;

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
            DiscordEvents::MessageCreate(message_create_bytes) => {
                match sonic_rs::from_slice::<Box<MessageCreate>>(&message_create_bytes) {
                    Ok(message_create) => Self::validate_message(&message_create),
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
    fn validate_message(message_create: &MessageCreate) -> Result<(), String> {
        if Self::bypass(message_create) {
            return Ok(());
        }

        let mut take_action = TakeAction {
            report: None,
            message: None,
            user: None,
        };

        if let Some(attachment_spam) = &CONTEXT.settings.read().unwrap().validations.attachment_spam
            && let Some(new_take_action) = Self::attachment_spam(attachment_spam, message_create)
        {
            Self::update_take_action(&mut take_action, new_take_action);
        }

        Self::take_action(&take_action, message_create)?;

        Ok(())
    }

    fn bypass(message_create: &MessageCreate) -> bool {
        if let Some(bypass) = &CONTEXT.settings.read().unwrap().bypass {
            if bypass.users.contains(&message_create.author.id.get()) {
                return true;
            }

            for member_role in &message_create.member.as_ref().unwrap().roles {
                if bypass.roles.contains(&member_role.get()) {
                    return true;
                }
            }
        }

        false
    }

    fn attachment_spam(
        attachment_spam: &PluginSettingsAttachmentSpam,
        message: &MessageCreate,
    ) -> Option<TakeAction> {
        if !message.content.is_empty() {
            return None;
        }

        let attachment_count = message.attachments.len();

        if attachment_count >= attachment_spam.count {
            let report = if attachment_spam.actions.report {
                Some(format!(
                    "Attachment spam ({attachment_count}), without message content"
                ))
            } else {
                None
            };

            return Some(TakeAction {
                report,
                message: attachment_spam.actions.message,
                user: attachment_spam.actions.user,
            });
        }

        None
    }

    fn update_take_action(take_action: &mut TakeAction, new_take_action: TakeAction) {
        if let Some(new_report) = new_take_action.report {
            if let Some(report) = &mut take_action.report {
                let _ = write!(report, "\n- {new_report}");
            } else {
                take_action.report = Some(format!("- {new_report}"));
            }
        }

        // This will need an update when other message actions get introduced
        if new_take_action.message.is_some() && take_action.message.is_none() {
            take_action.message = new_take_action.message;
        }

        //if let Some(new_message_action) = new_take_action.message {
        //    if let Some(message_action) = take_action.message {
        //        match message_action {
        //            ActionsMessage::Delete => (),
        //        }
        //    } else {
        //        take_action.message = new_take_action.message;
        //    }
        //}

        if let Some(new_user_action) = new_take_action.user {
            if let Some(user_action) = take_action.user {
                match user_action {
                    ActionsUser::Ban => (),
                    ActionsUser::TimeOut(period) => match new_user_action {
                        ActionsUser::Ban => take_action.user = new_take_action.user,
                        ActionsUser::TimeOut(new_period) => {
                            take_action.user = Some(ActionsUser::TimeOut(period + new_period));
                        }
                    },
                }
            } else {
                take_action.user = new_take_action.user;
            }
        }
    }

    fn take_action(take_action: &TakeAction, message: &MessageCreate) -> Result<(), String> {
        if let Some(message_action) = take_action.message {
            match message_action {
                ActionsMessage::Delete => Self::delete_message(message)?,
            }
        }

        if let Some(user_action) = take_action.user {
            match user_action {
                ActionsUser::Ban => Self::ban_user(take_action.report.as_deref(), message)?,
                ActionsUser::TimeOut(period) => Self::time_out_user(message, period)?,
            }
        }

        if take_action.report.is_some() {
            Self::report(take_action, message)?;
        }

        Ok(())
    }

    fn report(take_action: &TakeAction, message: &MessageCreate) -> Result<(), String> {
        let mut embed = Self::base_embed(message);

        embed.description = Some(format!(
            "**Reasons:**\n{}\n\n**Actions Taken:**",
            take_action.report.as_ref().unwrap()
        ));

        let embed_description = embed.description.as_mut().unwrap();

        if let Some(message_action) = take_action.message {
            match message_action {
                ActionsMessage::Delete => embed_description.push_str("\n- Message deleted"),
            }
        }

        if let Some(user_action) = take_action.user {
            match user_action {
                ActionsUser::Ban => embed_description.push_str("\n- User banned"),
                ActionsUser::TimeOut(period) => {
                    let _ = write!(embed_description, "\n- User timed out for {period} seconds",);
                }
            }
        }

        embed_description.push_str("\n\n**Message:**\n");

        if message.content.is_empty() {
            embed_description.push_str("No Content");
        } else {
            embed_description.push_str(&message.content);
        }

        embed_description.push('\n');

        if message.attachments.is_empty() {
            embed_description.push_str("\nNo Attachments");
        } else {
            for attachment in &message.attachments {
                embed_description.push('\n');
                embed_description.push_str(&attachment.url);
            }
        }

        let client = Client::builder().build();

        let create_message_request = match client
            .create_message(Id::new(CONTEXT.settings.read().unwrap().automod_channel_id))
            .embeds(&[embed])
            .try_into_request()
        {
            Ok(create_message_request) => create_message_request,
            Err(err) => {
                return Err(format!(
                    "An error occured while creating the report create message request: {err}"
                ));
            }
        };

        discord_request(&Requests::CreateMessage((
            CONTEXT.settings.read().unwrap().automod_channel_id,
            Contents::Json(create_message_request.body().unwrap().to_owned()),
        )))?;

        Ok(())
    }

    fn delete_message(message: &MessageCreate) -> Result<(), String> {
        discord_request(&Requests::DeleteMessage((
            message.channel_id.get(),
            message.id.get(),
        )))?;

        Ok(())
    }

    fn time_out_user(message: &MessageCreate, period: u64) -> Result<(), String> {
        let client = Client::builder().build();

        let update_member_request = match client
            .update_guild_member(message.guild_id.unwrap(), message.author.id)
            .communication_disabled_until(Some(
                Timestamp::from_secs(
                    (UNIX_EPOCH.elapsed().unwrap_or_default().as_secs() + period)
                        .try_into()
                        .unwrap_or_default(),
                )
                .unwrap_or(Timestamp::from_secs(0).unwrap()),
            ))
            .try_into_request()
        {
            Ok(update_member_request) => update_member_request,
            Err(err) => {
                return Err(format!(
                    "An error occured while creating the update member request: {err}"
                ));
            }
        };

        discord_request(&Requests::UpdateMember((
            message.guild_id.unwrap().get(),
            message.author.id.get(),
            update_member_request.body().unwrap().to_owned(),
        )))?;

        Ok(())
    }

    fn ban_user(reason: Option<&str>, message: &MessageCreate) -> Result<(), String> {
        let client = Client::builder().build();

        let create_ban_request = match client
            .create_ban(message.guild_id.unwrap(), message.author.id)
            .reason(reason.unwrap_or("No reason provided"))
            .try_into_request()
        {
            Ok(create_ban_request) => create_ban_request,
            Err(err) => {
                return Err(format!(
                    "An error occured while creating the create ban request: {err}"
                ));
            }
        };

        discord_request(&Requests::CreateBan((
            message.guild_id.unwrap().get(),
            message.author.id.get(),
            create_ban_request.body().unwrap().to_owned(),
        )))?;

        Ok(())
    }

    fn base_embed(message: &MessageCreate) -> Embed {
        Embed {
            author: Some(EmbedAuthor {
                icon_url: message.author.avatar.map(|avatar| {
                    format!(
                        "https://cdn.discordapp.com/avatars/{}/{}.webp",
                        message.author.id.get(),
                        avatar
                    )
                }),
                name: message.author.name.clone(),
                proxy_icon_url: None,
                url: None,
            }),
            color: Some(0x00E7_2323),
            description: None,
            fields: vec![],
            footer: Some(EmbedFooter {
                icon_url: None,
                proxy_icon_url: None,
                text: format!("ID: {}", message.author.id.get()),
            }),
            image: None,
            kind: String::from("rich"),
            provider: None,
            thumbnail: None,
            timestamp: Some(
                Timestamp::from_secs(
                    UNIX_EPOCH
                        .elapsed()
                        .unwrap_or_default()
                        .as_secs()
                        .try_into()
                        .unwrap_or_default(),
                )
                .unwrap_or(Timestamp::from_secs(0).unwrap()),
            ),
            title: Some(String::from("Automod Report")),
            url: None,
            video: None,
        }
    }
}

export!(Plugin);
