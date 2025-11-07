use std::time::Duration;

use floem::{
    action::debounce_action,
    prelude::{palette::css, *},
    reactive::Trigger,
    receiver_signal::Resource,
    style::{Background, CursorStyle, Transition},
    text::Weight,
};
use serde::Deserialize;
use tokio::runtime::Runtime;

#[derive(Debug, Clone, Deserialize)]
struct GitHubUser {
    login: Option<String>,
    name: Option<String>,
    bio: Option<String>,
    public_repos: u32,
    followers: u32,
    following: u32,
    avatar_url: String,
    html_url: String,
}

#[derive(Debug, Clone)]
struct UserWithAvatar {
    user: GitHubUser,
    avatar_data: Vec<u8>,
}

#[derive(Debug, Clone, thiserror::Error)]
enum ApiError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("User not found")]
    NotFound,
    #[error("Rate limit exceeded. Resets at {reset_time}")]
    RateLimit { reset_time: String },
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("Failed to parse response: {0}")]
    Parse(String),
}

type UserResult = Result<UserWithAvatar, ApiError>;

async fn fetch_github_user(username: String, token: Option<String>) -> UserResult {
    assert!(
        !username.is_empty(),
        "fetch_github_user called with empty username"
    );

    let url = format!("https://api.github.com/users/{}", username.trim());
    let client = reqwest::Client::new();
    let mut request = client.get(&url).header("User-Agent", "floem-github-search");

    // Add authorization header if token is provided
    if let Some(token) = &token
        && !token.trim().is_empty()
    {
        request = request.header("Authorization", format!("token {}", token.trim()));
    }

    let response = request
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?;

    let status = response.status();

    match status.as_u16() {
        200..=299 => {
            let user: GitHubUser = response
                .json()
                .await
                .map_err(|e| ApiError::Parse(format!("{e:?}")))?;

            // Use the same token for avatar request if available
            let mut avatar_request = client.get(&user.avatar_url);
            if let Some(token) = token
                && !token.trim().is_empty()
            {
                avatar_request =
                    avatar_request.header("Authorization", format!("token {}", token.trim()));
            }

            let avatar_response = avatar_request
                .send()
                .await
                .map_err(|e| ApiError::Network(format!("Failed to fetch avatar: {e}")))?;

            let avatar_data = avatar_response
                .bytes()
                .await
                .map_err(|e| ApiError::Network(format!("Failed to read avatar data: {e}")))?
                .to_vec();

            Ok(UserWithAvatar { user, avatar_data })
        }
        404 => Err(ApiError::NotFound),
        403 => {
            let headers = response.headers();
            if let Some(remaining) = headers.get("x-ratelimit-remaining")
                && remaining == "0"
            {
                let reset_time = headers
                    .get("x-ratelimit-reset")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<i64>().ok())
                    .map(|timestamp| {
                        use std::time::{SystemTime, UNIX_EPOCH};
                        let duration = std::time::Duration::from_secs(timestamp as u64);
                        let reset = UNIX_EPOCH + duration;
                        let now = SystemTime::now();
                        if let Ok(diff) = reset.duration_since(now) {
                            let mins = diff.as_secs() / 60;
                            format!("in {mins} minutes")
                        } else {
                            "soon".to_string()
                        }
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                return Err(ApiError::RateLimit { reset_time });
            }

            let error_msg = response
                .text()
                .await
                .ok()
                .unwrap_or_else(|| "Forbidden".to_string());

            Err(ApiError::Http {
                status: 403,
                message: error_msg,
            })
        }
        401 => Err(ApiError::Http {
            status: 401,
            message: "Invalid token or unauthorized access".to_string(),
        }),
        _ => {
            let error_msg = response
                .text()
                .await
                .ok()
                .and_then(|text| {
                    serde_json::from_str::<serde_json::Value>(&text)
                        .ok()
                        .and_then(|json| json["message"].as_str().map(String::from))
                        .or(Some(text))
                })
                .unwrap_or_else(|| {
                    status
                        .canonical_reason()
                        .unwrap_or("Unknown error")
                        .to_string()
                });

            Err(ApiError::Http {
                status: status.as_u16(),
                message: error_msg,
            })
        }
    }
}

fn stat_item(label: &str, value: u32) -> impl IntoView {
    let value_view = value
        .to_string()
        .style(|s| s.font_size(18.0).font_weight(Weight::BOLD));

    let label_view = label
        .to_string()
        .style(|s| s.font_size(12.0).color(css::GRAY));

    (value_view, label_view)
        .v_stack()
        .style(|s| s.items_center())
}

fn user_display(user_resource: Resource<Option<UserResult>>) -> impl IntoView {
    dyn_container(
        move || user_resource.get(),
        |result| match result {
            Some(Ok(user_with_avatar)) => {
                let user = &user_with_avatar.user;
                let avatar_data = user_with_avatar.avatar_data.clone();

                let avatar = img(move || avatar_data.clone())
                    .style(|s| s.size(60.0, 60.0))
                    .clip()
                    .style(|s| {
                        s.size(60.0, 60.0)
                            .border_radius(30.0)
                            .items_center()
                            .justify_center()
                    });

                let login = user
                    .login
                    .clone()
                    .unwrap_or("None".to_string())
                    .style(|s| s.font_size(20.0).font_weight(Weight::BOLD));

                let username = user
                    .name
                    .as_ref()
                    .map(|name| {
                        name.clone()
                            .style(|s| s.font_size(16.0).color(css::GRAY))
                            .into_any()
                    })
                    .unwrap_or_else(|| empty().into_any());

                let name_section = (login, username)
                    .v_stack()
                    .style(|s| s.items_start().justify_center());

                let header = (avatar, name_section)
                    .h_stack()
                    .style(|s| s.items_center().col_gap(15.0).margin_bottom(15.0));

                let bio = user
                    .bio
                    .as_ref()
                    .map(|bio| {
                        bio.clone()
                            .style(|s| s.font_size(14.0).margin_bottom(15.0).line_height(1.4))
                            .into_any()
                    })
                    .unwrap_or_else(|| empty().into_any());

                let stats = (
                    stat_item("Repos", user.public_repos),
                    stat_item("Followers", user.followers),
                    stat_item("Following", user.following),
                )
                    .h_stack()
                    .style(|s| s.col_gap(20.0).justify_center());

                let open_url = user.html_url.clone();
                let profile_link = button(format!("View on GitHub: {}", user.html_url))
                    .style(|s| {
                        s.font_size(12.0)
                            .color(css::BLUE)
                            .margin_top(15.0)
                            .cursor(CursorStyle::Pointer)
                    })
                    .action(move || {
                        let _ = open::that(open_url.clone());
                    });

                (header, bio, stats, profile_link).v_stack().into_any()
            }
            Some(Err(ApiError::NotFound)) => "User not found"
                .style(|s| s.color(css::RED).font_size(16.0))
                .into_any(),
            Some(Err(error)) => format!("Error: {error}")
                .style(|s| s.color(css::RED))
                .into_any(),
            None => "Enter a GitHub username to search"
                .style(|s| s.color(css::GRAY).font_style(floem::text::Style::Italic))
                .into_any(),
        },
    )
    .container()
    .style(|s| {
        s.padding(20.0)
            .border_radius(8.0)
            .border(1.0)
            .border_color(css::LIGHT_GRAY)
            .background(css::WHITE)
            .min_height(200.0)
            .min_width(400.0)
            .justify_center()
            .items_center()
    })
}

fn app_view() -> impl IntoView {
    let username = RwSignal::new(String::new());
    let token = RwSignal::new(String::new());

    // Create a Trigger for token changes
    let token_changed = Trigger::new();

    let get_username = Trigger::new();
    debounce_action(username, Duration::from_millis(200), move || {
        get_username.notify()
    });

    debounce_action(token, Duration::from_millis(200), move || {
        token_changed.notify()
    });

    let user_resource = Resource::new(
        move || (username.get(), token.get()),
        |(username, token): (String, String)| async move {
            if username.trim().is_empty() {
                return Err(ApiError::Http {
                    status: 400,
                    message: "Username cannot be empty".to_string(),
                });
            }
            let token_opt = if token.trim().is_empty() {
                None
            } else {
                Some(token)
            };
            fetch_github_user(username, token_opt).await
        },
    );

    let title = "GitHub User Search".style(|s| {
        s.font_size(28.0)
            .font_weight(Weight::BOLD)
            .margin_bottom(30.0)
    });

    let search_input = (
        "Username:".style(|s| s.font_size(14.0).margin_bottom(5.0)),
        text_input(username)
            .placeholder("e.g., octocat")
            .style(|s| {
                s.padding(8.0)
                    .border(1.0)
                    .border_color(css::GRAY)
                    .border_radius(4.0)
                    .width(300.0)
                    .focus(|s| s.border_color(css::BLUE))
            }),
    )
        .v_stack()
        .style(|s| s.items_start().margin_bottom(20.0));

    // Create token input with visibility toggle
    let token_input_container = {
        let token_label =
            "GitHub Token (optional):".style(|s| s.font_size(14.0).margin_bottom(5.0));

        let token_input = text_input(token)
            .placeholder("ghp_xxxxxxxxxxxxxxxxxxxx")
            .style(|s| {
                s.padding(8.0)
                    .border(1.0)
                    .border_color(css::GRAY)
                    .border_radius(4.0)
                    .width(300.0)
                    .focus(|s| s.border_color(css::BLUE))
            });

        let token_info =
            "Adding a token increases the rate limit and may grant access to private repositories."
                .style(|s| {
                    s.font_size(11.0)
                        .color(css::DARK_GRAY)
                        .margin_top(5.0)
                        .width(300.0)
                });

        (token_label, token_input, token_info)
            .v_stack()
            .style(|s| s.items_start().margin_bottom(20.0))
    };

    let loading_indicator = dyn_container(
        move || user_resource.is_loading(),
        |is_loading| {
            if is_loading {
                "Searching..."
                    .style(|s| s.color(css::BLUE).font_size(14.0).margin_bottom(10.0))
                    .into_any()
            } else {
                empty().into_any()
            }
        },
    );

    // Rate limit indicator
    let rate_limit_indicator = dyn_container(
        move || {
            if let Some(Err(ApiError::RateLimit { reset_time })) = user_resource.get() {
                Some(reset_time)
            } else {
                None
            }
        },
        |reset| {
            if let Some(reset_time) = reset {
                format!("Rate limit exceeded. Resets {reset_time}")
                    .style(|s| s.color(css::RED).font_size(14.0).margin_bottom(10.0))
                    .into_any()
            } else {
                empty().into_any()
            }
        },
    );

    let refresh_button = button("Refresh")
        .action(move || user_resource.refetch())
        .style(|s| {
            s.padding_vert(6.0)
                .padding_horiz(12.0)
                .border_radius(4.0)
                .background(css::GRAY)
                .color(css::WHITE)
                .margin_top(15.0)
                .transition(Background, Transition::ease_in_out(100.millis()))
                .hover(|s| s.background(css::DARK_GRAY).cursor(CursorStyle::Pointer))
        });

    let hint = "Type a username and it will automatically search".style(|s| {
        s.font_size(12.0)
            .color(css::GRAY)
            .margin_top(20.0)
            .font_style(floem::text::Style::Italic)
    });

    (
        title,
        search_input,
        token_input_container,
        loading_indicator,
        rate_limit_indicator,
        user_display(user_resource),
        refresh_button,
        hint,
    )
        .v_stack()
        .style(|s| {
            s.size_full()
                .items_center()
                .justify_center()
                .padding(20.0)
                .background(floem::peniko::Color::from_rgb8(248, 249, 250))
        })
        .window_title(|| "GitHub User Search".to_owned())
        .style(|s| {
            s.class(PlaceholderTextClass, |s| {
                s.color(css::BLACK.with_alpha(0.35))
            })
        })
}

fn main() {
    // Multi threaded runtime is required because the main thread is not a real tokio task
    let runtime = Runtime::new().expect("Could not start tokio runtime");

    // We must make it so that the main task is under the tokio runtime so that APIs like
    // tokio::spawn work
    runtime.block_on(async { tokio::task::block_in_place(|| floem::launch(app_view)) })
}
