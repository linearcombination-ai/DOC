from doc.utils.template_env import env
import smtplib
from os.path import basename
from typing import Optional
from doc.config import settings
from doc.domain.model import Attachment
from email.encoders import encode_base64
from email.mime.base import MIMEBase
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText

logger = settings.logger(__name__)


def should_send_email(
    # email_address comes in as pydantic.EmailStr and leaves
    # the pydantic class validator as a str.
    email_address: Optional[str],
    send_email: bool = settings.SEND_EMAIL,
) -> bool:
    """
    Return True if configuration is set to send email and the user
    has supplied an email address.
    """
    return send_email and email_address is not None


def send_email_with_attachment(
    # email_address comes in as pydantic.EmailStr and leaves
    # the pydantic class validator as a str.
    email_address: Optional[str],
    attachments: list[Attachment],
    document_request_key: str,
    content_disposition: str = "attachment",
    from_email_address: str = settings.FROM_EMAIL_ADDRESS,
    smtp_password: str = settings.SMTP_PASSWORD,
    email_send_subject: str = settings.EMAIL_SEND_SUBJECT,
    smtp_host: str = settings.SMTP_HOST,
    smtp_port: int = settings.SMTP_PORT,
    comma_space: str = ", ",
) -> None:
    """
    If environment configuration allows sending of
    email, then send an email to the document request
    recipient's email with the document attached.
    """
    if email_address:
        sender = from_email_address
        email_password = smtp_password
        recipients = [email_address]
        logger.debug("Email sender %s, recipients: %s", sender, recipients)
        # Create the enclosing (outer) message
        outer = MIMEMultipart()
        outer["Subject"] = email_send_subject
        outer["To"] = comma_space.join(recipients)
        outer["From"] = sender
        # Add the attachments to the message
        for attachment in attachments:
            try:
                with open(attachment.filepath, "rb") as fp:
                    msg = MIMEBase(attachment.mime_type[0], attachment.mime_type[1])
                    msg.set_payload(fp.read())
                encode_base64(msg)
                msg.add_header(
                    "Content-Disposition",
                    content_disposition,
                    filename=basename(attachment.filepath),
                )
                outer.attach(msg)
            except Exception:
                logger.exception(
                    "Unable to open one of the attachments. Caught exception: "
                )
        # Get the email body
        message_body = instantiated_email_template(document_request_key)
        logger.debug("instantiated email template: %s", message_body)
        outer.attach(MIMEText(message_body, "plain"))
        composed = outer.as_string()
        # Send the email
        try:
            with smtplib.SMTP(
                smtp_host, smtp_port, timeout=settings.SMTP_TIMEOUT_SECONDS
            ) as smtp:
                smtp.ehlo()
                smtp.starttls()
                smtp.ehlo()
                smtp.login(sender, email_password)
                smtp.sendmail(sender, recipients, composed)
                smtp.close()
            logger.info("Email sent!")
        except Exception:
            logger.exception("Unable to send the email. Caught exception: ")


def instantiated_email_template(document_request_key: str) -> str:
    template = env.get_template("text/email.txt")
    return template.render(data=document_request_key)
