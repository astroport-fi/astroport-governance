use crate::error::ContractError;
use crate::error::ContractError::MarketingInfoValidationError;

use cosmwasm_std::StdError;
use cw20::Logo;

const SAFE_TEXT_CHARS: &str = "!&?#()*+'-.,/\"";
const SAFE_LINK_CHARS: &str = "-_:/?#@!$&()*+,;=.~[]'%";

fn validate_text(text: &str, name: &str) -> Result<(), ContractError> {
    if text.chars().any(|c| {
        !c.is_ascii_alphanumeric() && !c.is_ascii_whitespace() && !SAFE_TEXT_CHARS.contains(c)
    }) {
        Err(MarketingInfoValidationError(format!(
            "{name} contains invalid characters: {text}"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_whitelist_links(links: &[String]) -> Result<(), ContractError> {
    links.iter().try_for_each(|link| {
        if !link.ends_with('/') {
            return Err(MarketingInfoValidationError(format!(
                "Whitelist link should end with '/': {link}"
            )));
        }
        validate_link(link)
    })
}

pub fn validate_link(link: &String) -> Result<(), ContractError> {
    if link
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && !SAFE_LINK_CHARS.contains(c))
    {
        Err(StdError::generic_err(format!("Link contains invalid characters: {link}")).into())
    } else {
        Ok(())
    }
}

fn check_link(link: &String, whitelisted_links: &[String]) -> Result<(), ContractError> {
    if validate_link(link).is_err() {
        Err(MarketingInfoValidationError(format!(
            "Logo link is invalid: {link}"
        )))
    } else if !whitelisted_links.iter().any(|wl| link.starts_with(wl)) {
        Err(MarketingInfoValidationError(format!(
            "Logo link is not whitelisted: {link}"
        )))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_marketing_info(
    project: Option<&String>,
    description: Option<&String>,
    logo: Option<&Logo>,
    whitelisted_links: &[String],
) -> Result<(), ContractError> {
    if let Some(description) = description {
        validate_text(description, "description")?;
    }
    if let Some(project) = project {
        validate_text(project, "project")?;
    }
    if let Some(Logo::Url(url)) = logo {
        check_link(url, whitelisted_links)?;
    }

    Ok(())
}
