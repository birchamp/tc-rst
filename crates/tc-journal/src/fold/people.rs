//! Who a person is on the screens that name people: the conflict review and the sync result
//! (§8.5; `docs/TC4-DELTA-2026-10-16.md` §3.9). One function gives both, so one teammate has
//! one name, or one number, in both places.

use super::FoldState;
use super::merge::record_of;
use super::state::Projected;
use crate::ids::{DeviceId, DeviceKind};
use crate::record::{DeviceProfileValue, Record};
use std::collections::{BTreeMap, BTreeSet};

/// How the screens name the person behind a device.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PersonLabel {
    /// This device's person (`conflicts.mine`): this device, and every device that signed in
    /// to Door43 with the same username.
    Mine,
    /// The Door43 username; every device with it is the same person.
    Username(Box<str>),
    /// The display name of a device with no username.
    DisplayName(Box<str>),
    /// "Teammate n" (`conflicts.teammate`): a device with neither, numbered from 1 among
    /// those devices in the order of their ids, leaving out this device.
    Teammate(u32),
}

impl FoldState {
    /// The label of every device that stands for a person and holds a change or a profile
    /// here: Local devices, tC4 actors and tC3 users. Devices that record what a file or a
    /// commit held (a Door43 edit, an exchange-file edit, an import) are no person; the
    /// screens name them by their cause. `me` is this device.
    pub fn person_labels(&self, me: DeviceId) -> BTreeMap<DeviceId, PersonLabel> {
        let person = |d: &DeviceId| {
            matches!(
                d.kind(),
                DeviceKind::Local | DeviceKind::Tc4Actor | DeviceKind::Tc3User
            )
        };
        let devices: BTreeSet<DeviceId> = self
            .devices
            .iter()
            .chain(self.profile_keys.iter().map(|(d, _)| d))
            .copied()
            .filter(person)
            .chain([me])
            .collect();
        let mine = self.profile(me).door43_username;
        let mut out = BTreeMap::new();
        let mut unnamed = 0u32;
        for device in devices {
            let profile = self.profile(device);
            let label = if device == me
                || mine.is_some() && profile.door43_username.as_ref() == mine.as_ref()
            {
                PersonLabel::Mine
            } else if let Some(name) = profile.door43_username {
                PersonLabel::Username(name.as_str().into())
            } else if let Some(name) = profile.display_name {
                PersonLabel::DisplayName(name)
            } else {
                unnamed += 1;
                PersonLabel::Teammate(unnamed)
            };
            out.insert(device, label);
        }
        out
    }

    /// The projected profile of `device`; an empty one when it has none.
    fn profile(&self, device: DeviceId) -> DeviceProfileValue {
        let Some(k) = self.profile_keys.get(&device) else {
            return DeviceProfileValue::default();
        };
        let record = match &self.entry(*k).projected {
            Projected::Change(c) => record_of(self, *c),
            Projected::Merged(record) => Some((**record).clone()),
            Projected::Nothing => None,
        };
        match record {
            Some(Record::DeviceProfile { value, .. }) => value,
            _ => DeviceProfileValue::default(),
        }
    }
}
