use async_graphql::Enum;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum Permission {
    WriteExternalStorage,
    ReadSms,
    SendSms,
    ReadContacts,
    WriteContacts,
    ReadCallLog,
    WriteCallLog,
    CallPhone,
    PostNotifications,
    NearbyWifiDevices,
    AccessFineLocation,
    Camera,
    SystemAlertWindow,
    RecordAudio,
    ReadMediaImages,
    ReadMediaVideos,
    ReadMediaAudio,
    NotificationListener,
    ReadPhoneState,
    ReadPhoneNumbers,
    ScheduleExactAlarm,
    QueryAllPackages,
    Adb,
    Clipboard,
}

impl Permission {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        <Self as async_graphql::EnumType>::items()
            .iter()
            .find(|item| item.name == value)
            .map(|item| item.value)
    }
}
