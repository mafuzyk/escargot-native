pub mod add_contact;
pub mod browser;
pub mod checkers;
pub mod chess;
pub mod assets;
pub mod contacts;
pub mod conversation;
pub mod games;
pub mod input;
pub mod login;
pub mod theme;
pub mod widgets;

use gpui::actions;

actions!(
    escargot,
    [
        SendMessage,
        SignIn,
        AddContact,
        SubmitContact,
        RefreshContacts,
        SignOut,
        NextTab,
        PreviousTab,
        CloseChat,
        OpenSettings
    ]
);

pub mod scale;
pub mod wallpaper;
