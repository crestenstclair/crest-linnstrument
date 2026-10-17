use super::*;

/// Linear synthetic pressure shaping, executed by the native clock in 1 ms
/// increments. Zero keeps the original immediate-contact behavior.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactProfile {
    pub attack_us: u32,
    pub release_us: u32,
}
impl ContactProfile {
    pub fn validate(self) -> Result<(), String> {
        if self.attack_us > 2_000_000 || self.release_us > 2_000_000 {
            return Err("contact attack/release must be within 0–2 seconds".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub(super) struct PressureRamp {
    pub id: u32,
    started: u64,
    ends: u64,
    from: f64,
    to: f64,
    pub next: u64,
    releasing: bool,
}
impl PressureRamp {
    fn new(
        id: u32,
        at: u64,
        duration: u32,
        from: f64,
        to: f64,
        releasing: bool,
    ) -> Result<Self, String> {
        if duration == 0 {
            return Err("zero pressure ramp duration".into());
        }
        let ends = at
            .checked_add(u64::from(duration))
            .ok_or("pressure ramp time overflow")?;
        Ok(Self {
            id,
            started: at,
            ends,
            from,
            to,
            next: at + u64::from(duration.min(1000)),
            releasing,
        })
    }
    fn value(self, at: u64) -> f64 {
        let fraction = at
            .saturating_sub(self.started)
            .min(self.ends - self.started) as f64
            / (self.ends - self.started) as f64;
        self.from + (self.to - self.from) * fraction
    }
    fn advance(&mut self, at: u64) {
        let elapsed = at
            .saturating_sub(self.started)
            .min(self.ends - self.started);
        self.next = self.started + ((elapsed / 1000 + 1) * 1000).min(self.ends - self.started);
    }
}
impl Runtime {
    pub(super) fn set_contact_profile(&mut self, profile: ContactProfile) -> Result<(), String> {
        profile.validate()?;
        if !self.contacts.is_empty() || self.scenario.is_some() {
            return Err(
                "release contacts and stop the script before changing pressure profile".into(),
            );
        }
        self.contact_profile = profile;
        Ok(())
    }
    pub(super) fn set_contact(&mut self, mut contact: Contact) -> Result<(), String> {
        scenario::validate_contact(contact, self.geometry)?;
        let old = self.contacts.iter().find(|c| c.id == contact.id).copied();
        if old.is_none() && self.contacts.len() >= 16 {
            // A released finger's synthetic tail must not prevent a new active
            // finger or make a previously validated script fail at capacity.
            let tail = self
                .ramps
                .iter()
                .filter(|r| r.releasing)
                .min_by_key(|r| r.started)
                .map(|r| r.id)
                .ok_or("16 contact limit")?;
            self.release_immediately(tail)?;
        }
        let previous_ramp = self.ramps.iter().find(|r| r.id == contact.id).copied();
        let duration = match previous_ramp {
            Some(ramp) if !ramp.releasing => ramp.ends.saturating_sub(self.firmware.at_us) as u32,
            Some(_) => self.contact_profile.attack_us,
            None if old.is_none() => self.contact_profile.attack_us,
            None => 0,
        };
        let ramp = if duration > 0 {
            let from = old.map_or(0.0, |c| c.z);
            let ramp = PressureRamp::new(
                contact.id,
                self.firmware.at_us,
                duration,
                from,
                contact.z,
                false,
            )?;
            contact.z = from;
            Some(ramp)
        } else {
            None
        };
        self.write_contact(contact)?;
        self.ramps.retain(|r| r.id != contact.id);
        if let Some(ramp) = ramp {
            self.ramps.push(ramp);
        }
        Ok(())
    }
    fn write_contact(&mut self, contact: Contact) -> Result<(), String> {
        self.request(&format!(
            "contact {} {} {} {}",
            contact.id, contact.x, contact.y, contact.z
        ))?;
        if let Some(old) = self.contacts.iter_mut().find(|c| c.id == contact.id) {
            *old = contact;
        } else {
            self.contacts.push(contact);
        }
        Ok(())
    }
    pub(super) fn release(&mut self, id: u32) -> Result<(), String> {
        if self.ramps.iter().any(|r| r.id == id && r.releasing) {
            return Ok(());
        }
        if self.contact_profile.release_us > 0
            && let Some(contact) = self.contacts.iter().find(|c| c.id == id)
        {
            let ramp = PressureRamp::new(
                id,
                self.firmware.at_us,
                self.contact_profile.release_us,
                contact.z,
                0.0,
                true,
            )?;
            self.ramps.retain(|r| r.id != id);
            self.ramps.push(ramp);
            return Ok(());
        }
        self.release_immediately(id)
    }
    pub(super) fn release_immediately(&mut self, id: u32) -> Result<(), String> {
        self.request(&format!("release {id}"))?;
        self.contacts.retain(|c| c.id != id);
        self.ramps.retain(|r| r.id != id);
        Ok(())
    }
    pub(super) fn advance_envelopes(&mut self) -> Result<(), String> {
        while let Some(index) = self
            .ramps
            .iter()
            .position(|r| r.next <= self.firmware.at_us)
        {
            let ramp = self.ramps[index];
            let at = self.firmware.at_us;
            if ramp.releasing && at >= ramp.ends {
                self.release_immediately(ramp.id)?;
                continue;
            }
            let mut contact = self
                .contacts
                .iter()
                .find(|c| c.id == ramp.id)
                .copied()
                .ok_or("pressure ramp lost contact")?;
            contact.z = ramp.value(at);
            self.write_contact(contact)?;
            if at >= ramp.ends {
                self.ramps.remove(index);
            } else {
                self.ramps[index].advance(at);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pressure_is_clock_based_bounded_and_wrap_safe() {
        let start = u64::from(u32::MAX) - 2000;
        let mut ramp = PressureRamp::new(1, start, 4000, 0.0, 0.8, false).unwrap();
        assert_eq!(ramp.value(start), 0.0);
        assert_eq!(ramp.value(start + 2000), 0.4);
        assert_eq!(ramp.value(start + 5000), 0.8);
        ramp.advance(start + 2300);
        assert_eq!(ramp.next, start + 3000);
        let release = PressureRamp::new(1, start, 4000, 0.8, 0.0, true).unwrap();
        assert!((release.value(start + 1000) - 0.6).abs() < 1e-12);
        assert!(PressureRamp::new(1, u64::MAX, 1, 0.0, 1.0, false).is_err());
        assert!(
            ContactProfile {
                attack_us: 2_000_001,
                release_us: 0
            }
            .validate()
            .is_err()
        );
    }
}
