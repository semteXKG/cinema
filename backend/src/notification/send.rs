use lettre::message::header::ContentType;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub struct EmailNotifier {
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from: String,
}

impl EmailNotifier {
    pub fn new(mailer: AsyncSmtpTransport<Tokio1Executor>, from: String) -> Self {
        EmailNotifier {
            mailer: Some(mailer),
            from,
        }
    }

    #[cfg(test)]
    fn new_unchecked(from: impl Into<String>) -> Self {
        EmailNotifier {
            mailer: None,
            from: from.into(),
        }
    }

    fn build_message(&self, to: &str, subject: &str, html: &str) -> anyhow::Result<Message> {
        Ok(Message::builder()
            .from(self.from.parse()?)
            .to(to.parse()?)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html.to_string())?)
    }

    pub async fn send(&self, to: &str, subject: &str, html: &str) -> anyhow::Result<()> {
        let email = self.build_message(to, subject, html)?;
        let mailer = self
            .mailer
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("email notifier has no transport"))?;
        mailer.send(email).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{EmailNotifier, Message};
    use lettre::message::header::Subject;

    fn build_test_email() -> Result<Message, anyhow::Error> {
        let notifier = EmailNotifier::new_unchecked("showings@example.com");
        notifier.build_message("user@example.com", "Subject", "<b>hi</b>")
    }

    #[test]
    fn builds_html_email_with_expected_headers() {
        let msg = build_test_email().unwrap();
        let headers = msg.headers().to_string();
        let body = String::from_utf8_lossy(&msg.formatted().to_vec()).to_string();
        assert!(headers.to_lowercase().contains("content-type: text/html"));
        assert!(headers.contains("showings@example.com"));
        assert!(headers.contains("user@example.com"));
        let subject = msg.headers().get::<Subject>().unwrap();
        assert!(subject.as_ref().contains("Subject"));
        assert!(body.contains("<b>hi</b>"));
    }
}
