from doc.utils.template_env import env
import smtplib
from os.path import basename
from typing import Optional
from doc.config import get_settings
from doc.domain.model import Attachment
from email.encoders import encode_base64
from email.mime.base import MIMEBase
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText

logger = get_settings().logger(__name__)


def should_send_email(
    # email_address comes in as pydantic.EmailStr and leaves
    # the pydantic class validator as a str.
    email_address: Optional[str],
) -> bool:
    """
    Return True if configuration is set to send email and the user
    has supplied an email address.
    """
    return get_settings().SEND_EMAIL and email_address is not None


def send_email_with_attachment(
    # email_address comes in as pydantic.EmailStr and leaves
    # the pydantic class validator as a str.
    email_address: Optional[str],
    attachments: list[Attachment],
    document_request_key: str,
    content_disposition: str = "attachment",
    comma_space: str = ", ",
) -> None:
    """
    If environment configuration allows sending of
    email, then send an email to the document request
    recipient's email with the document attached.
    """
    if email_address:
        settings = get_settings()
        sender = settings.FROM_EMAIL_ADDRESS
        email_password = settings.SMTP_PASSWORD
        recipients = [email_address]
        logger.debug("Email sender %s, recipients: %s", sender, recipients)
        # Create the enclosing (outer) message
        outer = MIMEMultipart()
        outer["Subject"] = settings.EMAIL_SEND_SUBJECT
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
            with smtplib.SMTP(settings.SMTP_HOST, settings.SMTP_PORT) as smtp:
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
