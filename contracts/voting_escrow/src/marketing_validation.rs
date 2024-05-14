use cosmwasm_std::ensure;
use cw20::Logo;

use crate::error::ContractError;

const SAFE_TEXT_CHARS: &str = "!&?#()*+'-.,/\"";
const SAFE_LINK_CHARS: &str = "-_:/?#@!$&()*+,;=.~[]'%";

pub fn validate_text(text: &str, name: &str) -> Result<(), ContractError> {
    if text.chars().any(|c| {
        !c.is_ascii_alphanumeric() && !c.is_ascii_whitespace() && !SAFE_TEXT_CHARS.contains(c)
    }) {
        Err(ContractError::MarketingInfoValidationError(format!(
            "{name} contains invalid characters: {text}"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_whitelist_links(links: &[String]) -> Result<(), ContractError> {
    links.iter().try_for_each(validate_link)
}

pub fn validate_link(link: &String) -> Result<(), ContractError> {
    ensure!(
        link.ends_with('/'),
        ContractError::MarketingInfoValidationError(format!(
            "Whitelist link should end with '/': {link}"
        ))
    );

    ensure!(
        link.chars()
            .all(|c| c.is_ascii_alphanumeric() || SAFE_LINK_CHARS.contains(c)),
        ContractError::MarketingInfoValidationError(format!(
            "Link contains invalid characters: {link}"
        ))
    );

    Ok(())
}

pub fn check_link(link: &String, whitelisted_links: &[String]) -> Result<(), ContractError> {
    validate_link(link)?;

    if !whitelisted_links.iter().any(|wl| link.starts_with(wl)) {
        Err(ContractError::MarketingInfoValidationError(format!(
            "Logo link is not whitelisted: {link}"
        )))
    } else {
        Ok(())
    }
}

pub fn validate_marketing_info(
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
