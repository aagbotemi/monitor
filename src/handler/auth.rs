use std::sync::Arc;

use axum::{
    extract::Query,
    http::{header, HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Redirect},
    routing::post,
    Extension, Json, Router,
};
use axum_extra::extract::cookie::Cookie;
use chrono::{Duration, Utc};
use validator::Validate;

use crate::{
    db::UserExt,
    dtos::{
        ForgotPasswordRequestDto, LoginUserDto, RegisterUserDto, ResetPasswordRequestDto, Response,
        UserLoginResponseDto, UserLogoutResponseDto, VerifyEmailQueryDto,
    },
    error::{ErrorMessage, HttpError},
    mail::mail::{send_forgot_password_email, send_verification_email, send_welcome_email},
    middleware::auth,
    utils::{password, token},
    AppState,
};

pub fn auth_handler() -> Router {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/verify", post(verify_email))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
        .route("/logout", post(logout).layer(middleware::from_fn(auth)))
}

pub async fn register(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(body): Json<RegisterUserDto>,
) -> Result<impl IntoResponse, HttpError> {
    body.validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    let verification_token = uuid::Uuid::new_v4().to_string();
    let token_expires_at = Utc::now() + Duration::hours(24);

    let hash_password =
        password::hash(&body.password).map_err(|e| HttpError::server_error(e.to_string()))?;

    let result = app_state
        .db_client
        .save_user(
            &body.name,
            &body.email,
            &hash_password,
            &verification_token,
            token_expires_at,
        )
        .await;

    match result {
        Ok(_user) => {
            let send_email_result =
                send_verification_email(&body.email, &body.name, &verification_token).await;

            if let Err(e) = send_email_result {
                eprint!("Failed to send verification email: {}", e);
            }

            Ok((
                StatusCode::CREATED,
                Json(Response {
                    status: "success",
                    message:
                        "Registration successful! Please check your email to verify your account"
                            .to_string(),
                }),
            ))
        }
        Err(sqlx::Error::Database(db_err)) => {
            if db_err.is_unique_violation() {
                Err(HttpError::unique_constraint_violation(
                    (ErrorMessage::EmailExist.to_string()),
                ))
            } else {
                Err(HttpError::server_error(db_err.to_string()))
            }
        }
        Err(e) => Err(HttpError::server_error(e.to_string())),
    }
}

pub async fn login(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(body): Json<LoginUserDto>,
) -> Result<impl IntoResponse, HttpError> {
    body.validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    let result = app_state
        .db_client
        .get_user(None, None, Some(&body.email), None)
        .await
        .map_err(|e| HttpError::server_error(e.to_string()))?;

    let user = result.ok_or(HttpError::bad_request(
        ErrorMessage::WrongCredential.to_string(),
    ))?;

    if !user.verified {
        return Err(HttpError::unauthorized(
            ErrorMessage::UserNotVerified.to_string(),
        ));
    }

    let password_matched = password::compare(&body.password, &user.password)
        .map_err(|_| HttpError::bad_request(ErrorMessage::WrongCredential.to_string()))?;

    if password_matched {
        let token = token::create_token(
            &user.id.to_string(),
            &app_state.env.jwt_secret.as_bytes(),
            app_state.env.jwt_maxage,
        )
        .map_err(|e| HttpError::server_error(e.to_string()))?;

        let cookie_duration = time::Duration::minutes(app_state.env.jwt_maxage * 60);

        let cookie = Cookie::build(token.clone())
            .path("/")
            .max_age(cookie_duration)
            .http_only(true)
            .build();

        let response = Json(UserLoginResponseDto {
            status: "success".to_string(),
            token,
        });

        let mut headers = HeaderMap::new();

        headers.append(header::SET_COOKIE, cookie.to_string().parse().unwrap());

        let mut response = response.into_response();
        response.headers_mut().extend(headers);

        Ok(response)
    } else {
        Err(HttpError::bad_request(
            ErrorMessage::WrongCredential.to_string(),
        ))
    }
}

pub async fn verify_email(
    Query(query_params): Query<VerifyEmailQueryDto>,
    Extension(app_state): Extension<Arc<AppState>>,
) -> Result<impl IntoResponse, HttpError> {
    query_params
        .validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    let result = app_state
        .db_client
        .get_user(None, None, None, Some(&query_params.token))
        .await
        .map_err(|e| HttpError::server_error(e.to_string()))?;

    let user = result.ok_or(HttpError::bad_request(
        ErrorMessage::InvalidToken.to_string(),
    ))?;

    if let Some(expires_at) = user.token_expires_at {
        if Utc::now() > expires_at {
            return Err(HttpError::bad_request(
                "Verification token has expired".to_string(),
            ))?;
        }
    } else {
        return Err(HttpError::bad_request(
            "Invalid verification token".to_string(),
        ))?;
    }
    app_state
        .db_client
        .verifed_token(&query_params.token)
        .await
        .map_err(|e| HttpError::server_error(e.to_string()))?;

    let send_welcome_email_result = send_welcome_email(&user.email, &user.name).await;

    if let Err(e) = send_welcome_email_result {
        eprint!("Faied to send welcome email: {:?}", e);
    }

    let token = token::create_token(
        &user.id.to_string(),
        &app_state.env.jwt_secret.as_bytes(),
        app_state.env.jwt_maxage,
    )
    .map_err(|e| HttpError::server_error(e.to_string()))?;

    let cookie_duration = time::Duration::minutes(app_state.env.jwt_maxage * 60);

    let cookie = Cookie::build(token.clone())
        .path("/")
        .max_age(cookie_duration)
        .http_only(true)
        .build();

    let mut headers = HeaderMap::new();

    headers.append(header::SET_COOKIE, cookie.to_string().parse().unwrap());

    let frontend_url = format!("http://localhost:5173/settings");

    let redirect = Redirect::to(&frontend_url);

    let mut response = redirect.into_response();
    response.headers_mut().extend(headers);

    Ok(response)
}

pub async fn forgot_password(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(body): Json<ForgotPasswordRequestDto>,
) -> Result<impl IntoResponse, HttpError> {
    body.validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    let result = app_state
        .db_client
        .get_user(None, None, Some(&body.email), None)
        .await
        .map_err(|e| HttpError::server_error(e.to_string()))?;

    let user = result.ok_or(HttpError::bad_request("Email not found!".to_string()))?;

    let verification_token = uuid::Uuid::new_v4().to_string();
    let token_expires_at = Utc::now() + Duration::hours(24);

    let user_id = uuid::Uuid::parse_str(&user.id.to_string()).unwrap();

    app_state
        .db_client
        .add_verifed_token(user_id, &verification_token, token_expires_at)
        .await
        .map_err(|e| HttpError::server_error(e.to_string()))?;

    let reset_link = format!(
        "http://localhost:5173/reset-password?token={}",
        &verification_token
    );

    let email_sent = send_forgot_password_email(&user.email, &user.name, &reset_link).await;

    if let Err(e) = email_sent {
        eprint!("Failed to send forgot password email: {}", e);
        return Err(HttpError::server_error(
            "Failed to send forgot password email!".to_string(),
        ));
    }

    let response = Response {
        message: "Password reset link has been sent to your email!".to_string(),
        status: "success",
    };
    Ok(Json(response))
}

pub async fn reset_password(
    Extension(app_state): Extension<Arc<AppState>>,
    Json(body): Json<ResetPasswordRequestDto>,
) -> Result<impl IntoResponse, HttpError> {
    body.validate()
        .map_err(|e| HttpError::bad_request(e.to_string()))?;

    let result = app_state
        .db_client
        .get_user(None, None, None, Some(&body.token))
        .await
        .map_err(|e| HttpError::server_error(e.to_string()))?;

    let user = result.ok_or(HttpError::bad_request(
        "Invalid or expired token".to_string(),
    ))?;

    if let Some(expires_at) = user.token_expires_at {
        if Utc::now() > expires_at {
            return Err(HttpError::bad_request(
                "Verification token has expired".to_string(),
            ))?;
        }
    } else {
        return Err(HttpError::bad_request(
            "Invalid verification token".to_string(),
        ))?;
    }

    let user_id = uuid::Uuid::parse_str(&user.id.to_string()).unwrap();

    let hash_password =
        password::hash(&body.new_password).map_err(|e| HttpError::server_error(e.to_string()))?;

    app_state
        .db_client
        .update_user_password(user_id, hash_password)
        .await
        .map_err(|e| HttpError::server_error(e.to_string()))?;

    app_state
        .db_client
        .verifed_token(&body.token)
        .await
        .map_err(|e| HttpError::server_error(e.to_string()))?;

    let response = Response {
        message: "Password has been successfully reset!".to_string(),
        status: "success",
    };
    Ok(Json(response))
}

pub async fn logout() -> Result<impl IntoResponse, HttpError> {
    let cookie = Cookie::build("")
        .path("/")
        .max_age(time::Duration::minutes(-1))
        // .same_site(SameSite::Lax)
        .http_only(true)
        .build();

    let response = Json(UserLogoutResponseDto {
        status: "success".to_string(),
    });

    let mut headers = HeaderMap::new();

    headers.append(header::SET_COOKIE, cookie.to_string().parse().unwrap());

    let mut response = response.into_response();
    response.headers_mut().extend(headers);

    Ok(response)
}

// fn generate_random_string(length: usize) -> String {
//     let rng = rand::thread_rng();
//     let random_string: String = rng
//         .sample_iter(&Alphanumeric)
//         .take(length)
//         .map(char::from)
//         .collect();

//     random_string
// }
