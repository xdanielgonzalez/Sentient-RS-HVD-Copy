use super::prelude::*;

impl<const DEPTH: usize> CanonicalStore<DEPTH> {
    /// An empty store: no readings, no fault, nothing pending. `const` — usable as a `static`
    /// initializer on the MCU.
    ///
    /// # Example
    ///
    /// ```
    /// use lib_store::CanonicalStore;
    ///
    /// let store: CanonicalStore<4> = CanonicalStore::new();
    /// assert!(store.is_empty());
    /// assert_eq!(store.len(), 0);
    /// assert!(store.latest_reading().is_none());
    /// assert!(store.latest_fault().is_none());
    /// ```
    pub const fn new() -> Self {
        CanonicalStore {
            readings: HistoryBuf::new(),
            fault: None,
            pending: heapless::Vec::new(),
        }
    }

    /// Push a fresh reading (overwrites the oldest slot once `DEPTH` is reached), and mark both
    /// telemetry topics pending. Idempotent marking: if a topic is already pending (the previous
    /// reading hasn't been rendered yet), it is not duplicated in the pending list.
    ///
    /// # Example
    ///
    /// Bounded overwrite: a depth-3 store keeps only the 3 most recent readings.
    ///
    /// ```
    /// use lib_store::CanonicalStore;
    /// # fn reading(total_voltage_cv: u16) -> lib_store::CanonicalReading {
    /// #     lib_store::CanonicalReading {
    /// #         device_address: 0,
    /// #         cell_voltages_mv: heapless::Vec::new(),
    /// #         temperatures_raw: heapless::Vec::new(),
    /// #         current_ca: 0,
    /// #         total_voltage_cv,
    /// #         remaining_capacity_cah: 0,
    /// #         typical_capacity_cah: 0,
    /// #         rated_capacity_cah: 0,
    /// #         soc_dpct: 0,
    /// #         soh_dpct: 0,
    /// #         cycles: 0,
    /// #         port_voltage_cv: 0,
    /// #     }
    /// # }
    ///
    /// let mut store: CanonicalStore<3> = CanonicalStore::new();
    /// for cv in [5_301, 5_302, 5_303, 5_304, 5_305] {
    ///     store.push_reading(reading(cv)); // reading(cv): a CanonicalReading at that voltage
    /// }
    ///
    /// // 5 pushes into 3 slots: the 2 oldest were overwritten; the newest survives.
    /// assert_eq!(store.len(), 3);
    /// assert_eq!(store.latest_reading().map(|r| r.total_voltage_cv), Some(5_305));
    /// ```
    pub fn push_reading(&mut self, reading: CanonicalReading) {
        self.readings.write(reading);
        self.mark_pending(PendingTopic::DcMeasurementUpdate);
        self.mark_pending(PendingTopic::PowerHardwareUpdate);
    }

    /// Set the latest fault snapshot, and mark the diagnostic topic pending. Each snapshot
    /// replaces the previous one wholesale — the store keeps only the latest.
    ///
    /// # Example
    ///
    /// ```
    /// use lib_store::{CanonicalFault, CanonicalStore, PendingTopic};
    ///
    /// let mut store: CanonicalStore<4> = CanonicalStore::new();
    /// store.set_fault(CanonicalFault {
    ///     device_address: 0x00,
    ///     high_voltage_protection: true,
    ///     ..Default::default()
    /// });
    /// assert_eq!(
    ///     store.latest_fault().map(|f| f.high_voltage_protection),
    ///     Some(true)
    /// );
    /// assert_eq!(
    ///     store.take_pending().as_slice(),
    ///     &[PendingTopic::ActiveDiagnosticState]
    /// );
    ///
    /// // A later all-clear snapshot replaces it — latest wins.
    /// store.set_fault(CanonicalFault::default());
    /// assert_eq!(
    ///     store.latest_fault().map(|f| f.high_voltage_protection),
    ///     Some(false)
    /// );
    /// ```
    pub fn set_fault(&mut self, fault: CanonicalFault) {
        self.fault = Some(fault);
        self.mark_pending(PendingTopic::ActiveDiagnosticState);
    }

    /// The most recently pushed reading, or `None` if the store is empty.
    ///
    /// # Example
    ///
    /// ```
    /// use lib_store::CanonicalStore;
    /// # fn reading(total_voltage_cv: u16) -> lib_store::CanonicalReading {
    /// #     lib_store::CanonicalReading {
    /// #         device_address: 0,
    /// #         cell_voltages_mv: heapless::Vec::new(),
    /// #         temperatures_raw: heapless::Vec::new(),
    /// #         current_ca: 0,
    /// #         total_voltage_cv,
    /// #         remaining_capacity_cah: 0,
    /// #         typical_capacity_cah: 0,
    /// #         rated_capacity_cah: 0,
    /// #         soc_dpct: 0,
    /// #         soh_dpct: 0,
    /// #         cycles: 0,
    /// #         port_voltage_cv: 0,
    /// #     }
    /// # }
    ///
    /// let mut store: CanonicalStore<4> = CanonicalStore::new();
    /// assert!(store.latest_reading().is_none());
    ///
    /// store.push_reading(reading(5_301)); // reading(cv): a CanonicalReading at that voltage
    /// store.push_reading(reading(5_302));
    /// assert_eq!(store.latest_reading().map(|r| r.total_voltage_cv), Some(5_302));
    /// ```
    pub fn latest_reading(&self) -> Option<&CanonicalReading> {
        self.readings.recent()
    }

    /// The most recently set fault snapshot, or `None` if none has been set yet.
    pub fn latest_fault(&self) -> Option<&CanonicalFault> {
        self.fault.as_ref()
    }

    /// How many readings are currently held (`<= DEPTH`).
    pub fn len(&self) -> usize {
        self.readings.len()
    }

    /// True if no reading has been pushed yet.
    pub fn is_empty(&self) -> bool {
        self.readings.is_empty()
    }

    /// Mark `topic` pending, unless it is already in the pending list. A full pending list (all 3
    /// topics already pending — the only way this can happen, since the list's capacity equals the
    /// number of distinct `PendingTopic` variants) silently drops the mark: the topic is already
    /// going to be rendered on the next drain, so nothing is lost.
    fn mark_pending(&mut self, topic: PendingTopic) {
        if !self.pending.contains(&topic) {
            let _ = self.pending.push(topic);
        }
    }

    /// Drain and return every currently pending topic, clearing the pending list. The caller (the
    /// broker pump, in firmware) is expected to render and publish/retain each one; a topic not
    /// consumed here is lost (this is a "what's new since last drain" signal, not a durable queue)
    /// — matching the retained-topic semantics the underlying MQTT broker already provides (a late
    /// subscriber gets the latest retained value, not a history of every intermediate one).
    ///
    /// # Example
    ///
    /// Draining resets the "what's new" signal, and repeated writes to the same topics between
    /// drains are marked once, not duplicated.
    ///
    /// ```
    /// use lib_store::{CanonicalStore, PendingTopic};
    /// # fn reading(total_voltage_cv: u16) -> lib_store::CanonicalReading {
    /// #     lib_store::CanonicalReading {
    /// #         device_address: 0,
    /// #         cell_voltages_mv: heapless::Vec::new(),
    /// #         temperatures_raw: heapless::Vec::new(),
    /// #         current_ca: 0,
    /// #         total_voltage_cv,
    /// #         remaining_capacity_cah: 0,
    /// #         typical_capacity_cah: 0,
    /// #         rated_capacity_cah: 0,
    /// #         soc_dpct: 0,
    /// #         soh_dpct: 0,
    /// #         cycles: 0,
    /// #         port_voltage_cv: 0,
    /// #     }
    /// # }
    ///
    /// let mut store: CanonicalStore<4> = CanonicalStore::new();
    /// store.push_reading(reading(5_301)); // reading(cv): a CanonicalReading at that voltage
    /// store.push_reading(reading(5_302)); // same topics again — no duplicate pending entries
    ///
    /// let pending = store.take_pending();
    /// assert_eq!(pending.len(), 2);
    /// assert!(pending.contains(&PendingTopic::DcMeasurementUpdate));
    /// assert!(pending.contains(&PendingTopic::PowerHardwareUpdate));
    ///
    /// // Drained: nothing pending until the next write.
    /// assert!(store.take_pending().is_empty());
    /// ```
    pub fn take_pending(&mut self) -> PendingTopics {
        core::mem::take(&mut self.pending)
    }
}

impl<const DEPTH: usize> Default for CanonicalStore<DEPTH> {
    fn default() -> Self {
        Self::new()
    }
}
