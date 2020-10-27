use memmap::MmapMut;

use super::error::PinError;
use crate::OdroidC2Error;
use crate::{pin_map::PinId, OdroidResult};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;

#[derive(Debug)]
pub struct Memory {
    map: MmapMut,
    pin_leases: Mutex<HashMap<PinId, (usize, usize)>>,
}

impl Memory {
    pub fn new(map: MmapMut) -> Self {
        Self {
            map,
            pin_leases: Mutex::new(HashMap::with_capacity(40)),
        }
    }

    pub fn lease_input(&self, pin_id: PinId) -> OdroidResult<()> {
        use PinError::*;
        let mut leases = self
            .pin_leases
            .lock()
            .map_err(|_| LeaseMapPoisoned)
            .map_err(OdroidC2Error::PinError)?;
        let (current_input_leases, current_output_leases) = leases.entry(pin_id).or_insert((0, 0));

        if *current_output_leases == 0 {
            *current_input_leases += 1;
            Ok(())
        } else {
            Err(OdroidC2Error::PinError(WrongLease))
        }
    }

    pub fn lease_output(&self, pin_id: PinId) -> OdroidResult<()> {
        use PinError::*;
        let mut leases = self
            .pin_leases
            .lock()
            .map_err(|_| LeaseMapPoisoned)
            .map_err(OdroidC2Error::PinError)?;
        let (current_input_leases, current_output_leases) = leases.entry(pin_id).or_insert((0, 0));

        if *current_input_leases == 0 {
            *current_output_leases += 1;
            Ok(())
        } else {
            Err(OdroidC2Error::PinError(WrongLease))
        }
    }

    pub fn release_input(&self, pin_id: PinId) -> OdroidResult<()> {
        use PinError::*;
        let mut leases = self
            .pin_leases
            .lock()
            .map_err(|_| LeaseMapPoisoned)
            .map_err(OdroidC2Error::PinError)?;
        let (current_input_leases, _) = leases.get_mut(&pin_id).unwrap();
        *current_input_leases -= 1;

        Ok(())
    }

    pub fn release_output(&self, pin_id: PinId) -> OdroidResult<()> {
        use PinError::*;
        let mut leases = self
            .pin_leases
            .lock()
            .map_err(|_| LeaseMapPoisoned)
            .map_err(OdroidC2Error::PinError)?;

        let (_, current_output_leases) = leases.get_mut(&pin_id).unwrap();
        *current_output_leases -= 1;
        Ok(())
    }
}

impl Deref for Memory {
    type Target = MmapMut;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl DerefMut for Memory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}
