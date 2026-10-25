//! The Door43 username in this device's profile (Q7, tC4 #680, #161): written once, after a
//! signed-in sync and the name notice (D98); a name copied from another device is replaced by
//! the first signed-in sync; after that, whoever signs in, it never changes.

use tc_door43::Session;
use tc_journal::Record;
use tc_journal::command::{Command, PlannedChange, UpgradePlan};
use tc_journal::record::{DeviceProfileValue, Door43Username};
use tc_store::{ProjectHandle, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum UsernameError {
    #[error("the project has never been synced from this computer")]
    NotSynced,
    #[error("{0:?} is not a Door43 username")]
    NotAUsername(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Refused(String),
}

/// Writes `session`'s username into this device's profile when the person accepted the name
/// notice and a sync of this project has fetched. Returns whether it wrote anything.
pub async fn record_username(
    project: &ProjectHandle,
    session: &Session,
) -> Result<bool, UsernameError> {
    let Some(mut file) = project.sync_file().await? else {
        return Err(UsernameError::NotSynced);
    };
    if file.confirmed.is_none() {
        return Err(UsernameError::NotSynced);
    }
    let device = project.facts().device;
    if file.username_device.as_deref() == Some(device.to_string().as_str()) {
        return Ok(false);
    }
    let name = Door43Username::parse(session.username.as_str())
        .map_err(|_| UsernameError::NotAUsername(session.username.as_str().to_owned()))?;
    let key = tc_journal::RecordKey::DeviceProfile(device);
    let snapshot = project.snapshot();
    let mut value = match snapshot.fold().projected(&key) {
        Some(Record::DeviceProfile { value, .. }) => value,
        _ => DeviceProfileValue::default(),
    };
    let wrote = value.door43_username.as_ref() != Some(&name);
    if wrote {
        value.door43_username = Some(name);
        project
            .submit(Command::ApplyUpgrade {
                plan: UpgradePlan {
                    changes: vec![PlannedChange {
                        record: Record::DeviceProfile { device, value },
                        parents: snapshot.fold().heads(&key),
                        cause: tc_journal::Cause::Edit,
                    }],
                },
            })
            .await
            .map_err(|e| UsernameError::Refused(e.to_string()))?;
    }
    file.username_device = Some(device.to_string());
    project.set_sync_file(file).await?;
    Ok(wrote)
}
