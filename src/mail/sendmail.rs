use std::{env, fs};

use lettre::{
    message::{header, SinglePart},
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};

pub async fn send_email(
    to_email: &str,
    subject: &str,
    template_path: &str,
    placeholders: &[(String, String)],
) -> Result<(), Box<dyn std::error::Error>> {
    let smtp_username = env::var("SMTP_USERNAME")?;
    let smtp_password = env::var("SMTP_PASSWORD")?;
    let smtp_server = env::var("SMTP_SERVER")?;
    // let smtp_port = env::var("SMTP_PORT")?.parse()?;

    let mut html_template = fs::read_to_string(template_path)?;

    for (key, value) in placeholders {
        html_template = html_template.replace(key, &value);
    }

    let email = Message::builder()
        .from(smtp_username.parse()?)
        .to(to_email.parse()?)
        .subject(subject)
        .header(header::ContentType::TEXT_HTML)
        .singlepart(
            SinglePart::builder()
                .header(header::ContentType::TEXT_HTML)
                .body(html_template),
        )?;

    let credentials = Credentials::new(smtp_username.to_owned(), smtp_password.to_owned());

    // let mailer = SmtpTransport::starttls_relay(&smtp_server)?
    //     .credentials(credentials)
    //     .port(smtp_port)
    //     .build();
    let mailer = SmtpTransport::relay(&smtp_server)
        .unwrap()
        .credentials(credentials)
        .build();

    let result = mailer.send(&email);

    match result {
        Ok(_) => println!("Email sent successfully!"),
        Err(e) => println!("Failed to send email: {:?}", e),
    }

    Ok(())
}
