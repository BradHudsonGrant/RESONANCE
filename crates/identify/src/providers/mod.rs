pub mod acoustid;
pub mod audd;
pub mod bandcamp;
pub mod deezer;
pub mod discogs;
pub mod itunes;
pub mod musicbrainz;
pub mod soundcloud;
pub mod youtube;

pub use acoustid::AcoustId;
pub use audd::AudD;
pub use bandcamp::Bandcamp;
pub use deezer::Deezer;
pub use discogs::Discogs;
pub use itunes::Itunes;
pub use musicbrainz::MusicBrainz;
pub use soundcloud::SoundCloud;
pub use youtube::YouTube;

/// Every provider's ID, so a stored step report can be read back to the provider it names.
pub const PROVIDER_IDS: [&str; 9] = [
    acoustid::ID,
    audd::ID,
    bandcamp::ID,
    deezer::ID,
    discogs::ID,
    itunes::ID,
    musicbrainz::ID,
    soundcloud::ID,
    youtube::ID,
];
