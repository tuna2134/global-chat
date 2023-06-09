use poise::serenity_prelude as serenity;
use sqlx::sqlite::SqlitePool;

struct Data {
    pool: SqlitePool,
}
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

/// グローバルチャットに参加します。
#[poise::command(slash_command)]
async fn join(ctx: Context<'_>) -> Result<(), Error> {
    let pool = &ctx.data().pool;
    let channel_id = ctx.channel_id().0 as i64;
    let channel = sqlx::query!("SELECT * FROM Channels WHERE ChannelId = ?", channel_id)
        .fetch_optional(pool)
        .await;
    if let Ok(Some(_)) = channel {
        ctx.say("Channel already in database").await?;
        return Ok(());
    }
    sqlx::query!("INSERT INTO Channels VALUES (?)", channel_id)
        .execute(pool)
        .await
        .expect("Failed to insert channel into database");
    ctx.say("Channel added to database").await?;
    Ok(())
}

// Leave from global chat
#[poise::command(slash_command)]
async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let pool = &ctx.data().pool;
    let channel_id = ctx.channel_id().0 as i64;
    let channel = sqlx::query!("SELECT * FROM Channels WHERE ChannelId = ?", channel_id)
        .fetch_optional(pool)
        .await;
    if let Ok(Some(_)) = channel {
        sqlx::query!("DELETE FROM Channels WHERE ChannelId = ?", channel_id)
            .execute(pool)
            .await?;
        ctx.say("Leave from GlobalChat").await?;
        return Ok(());
    }
    ctx.say("You are't register yet.").await?;
    Ok(())
}

async fn all_event_handler(
    ctx: &serenity::Context,
    event: &poise::Event<'_>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        poise::Event::Message { new_message } => {
            let msg = new_message;
            if msg.author.bot {
                return Ok(());
            }
            let pool = &data.pool;
            let from_ch_id = msg.channel_id.0 as i64;
            let ch = sqlx::query!("SELECT * FROM Channels WHERE ChannelId = ?", from_ch_id)
                .fetch_one(pool)
                .await;
            if ch.is_err() {
                return Ok(());
            }
            let channels = sqlx::query!("SELECT * FROM Channels")
                .fetch_all(pool)
                .await
                .expect("Failed to fetch channels from database");
            for channel in channels {
                let channel_id = channel.ChannelId.unwrap() as u64;
                if channel_id == msg.channel_id.0 {
                    continue;
                };
                match ctx.cache.guild_channel(channel_id) {
                    Some(channel) => {
                        if channel.is_text_based() {
                            let webhooks = channel.webhooks(ctx).await?;
                            let mut webhook: Option<serenity::Webhook> = None;
                            for w in webhooks {
                                if w.name == Some("gc-webhook".to_string()) {
                                    webhook = Some(w);
                                    break;
                                }
                            }
                            if let Some(webhook) = webhook {
                                webhook
                                    .execute(&ctx.http, false, |w| {
                                        w.content(msg.content.clone());
                                        w.username(msg.author.name.clone());
                                        w.avatar_url(msg.author.avatar_url().unwrap());
                                        w
                                    })
                                    .await?;
                            } else {
                                let webhook =
                                    channel.create_webhook(&ctx.http, "gc-webhook").await?;
                                webhook
                                    .execute(&ctx.http, false, |w| {
                                        w.content(msg.content.clone());
                                        w.username(msg.author.name.clone());
                                        w.avatar_url(msg.author.avatar_url().unwrap());
                                        w
                                    })
                                    .await?;
                            }
                        }
                    }
                    None => {
                        let delete_channel_id = channel.ChannelId.unwrap();
                        sqlx::query!(
                            "DELETE FROM Channels WHERE ChannelId = ?",
                            delete_channel_id
                        )
                        .execute(&data.pool)
                        .await?;
                    }
                }
            }
        }
        poise::Event::Ready { data_about_bot } => {
            println!("{} is ready!", data_about_bot.user.name);
        }
        _ => {}
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    env_logger::init();
    dotenv::dotenv().ok();
    let pool = SqlitePool::connect((std::env::var("DATABASE_URL").unwrap()).as_str())
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let mut intents = serenity::GatewayIntents::non_privileged();
    intents.insert(serenity::GatewayIntents::GUILD_MESSAGES);
    intents.insert(serenity::GatewayIntents::MESSAGE_CONTENT);
    println!("Now booting...");
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![join(), leave()],
            event_handler: |ctx, event, _framework, data| {
                Box::pin(all_event_handler(ctx, event, data))
            },
            ..Default::default()
        })
        .token(std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"))
        .intents(intents)
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data { pool: pool.clone() })
            })
        });
    framework.run().await.unwrap();
}
