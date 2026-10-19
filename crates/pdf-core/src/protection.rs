//! Giving a file a password to open it, restrictions, or both (Protect, section 6.9,
//! ADR 0026), and reading the protection a file has.
//!
//! Lectrix writes AES-256 only (security handler revision 6, as Acrobat X and later), with
//! Acrobat's choices of restrictions. Encryption is written on a full save
//! ([`crate::save::Encrypt`]): an incremental update cannot add or change it.

use std::fmt;
use std::path::Path;

use mupdf::pdf::{Encryption, PdfDocument, PdfWriteOptions, Permission};

use crate::docinfo::{self, PdfA};
use crate::error::{Error, Result};
use crate::ffi::{crypt_info, open_pdf_shared};
use crate::save::{Encrypt, SaveKind, save_atomic_encrypted};

/// What a file allows printing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Printing {
    NotAllowed,
    /// At low resolution only (bit 3 without bit 12; Acrobat says 150 dpi).
    LowResolution,
    HighResolution,
}

/// What a file allows changing, as Acrobat's choices name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Changes {
    None,
    /// Inserting, deleting and rotating pages.
    PageAssembly,
    /// Filling in forms and signing existing signature fields.
    FormsAndSigning,
    /// Commenting, filling in forms and signing.
    CommentsFormsAndSigning,
    /// Anything but extracting pages.
    AnyExceptExtracting,
}

/// What a file allows to whoever opens it without its permissions password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Restrictions {
    pub printing: Printing,
    pub changes: Changes,
    /// Copying text, images and other content.
    pub copying: bool,
}

impl Restrictions {
    /// Nothing restricted.
    pub const NONE: Restrictions = Restrictions {
        printing: Printing::HighResolution,
        changes: Changes::AnyExceptExtracting,
        copying: true,
    };

    /// The permission bits Acrobat sets for these choices. Text for screen readers (bit
    /// 10) is always allowed, as PDF 2.0 says it is.
    pub fn permissions(self) -> Permission {
        let mut p = Permission::ACCESSIBILITY;
        p |= match self.printing {
            Printing::NotAllowed => Permission::empty(),
            Printing::LowResolution => Permission::PRINT,
            Printing::HighResolution => Permission::PRINT | Permission::PRINT_HQ,
        };
        p |= match self.changes {
            Changes::None => Permission::empty(),
            Changes::PageAssembly => Permission::ASSEMBLE,
            Changes::FormsAndSigning => Permission::FORM,
            Changes::CommentsFormsAndSigning => Permission::ANNOTATE | Permission::FORM,
            Changes::AnyExceptExtracting => {
                Permission::MODIFY | Permission::ANNOTATE | Permission::FORM | Permission::ASSEMBLE
            }
        };
        if self.copying {
            p |= Permission::COPY;
        }
        p
    }

    /// The choices nearest to permission bits another app wrote: the widest choice of
    /// changes whose telling bit is set.
    pub fn from_permissions(p: Permission) -> Restrictions {
        let printing = if !p.contains(Permission::PRINT) {
            Printing::NotAllowed
        } else if p.contains(Permission::PRINT_HQ) {
            Printing::HighResolution
        } else {
            Printing::LowResolution
        };
        let changes = if p.contains(Permission::MODIFY) {
            Changes::AnyExceptExtracting
        } else if p.contains(Permission::ANNOTATE) {
            Changes::CommentsFormsAndSigning
        } else if p.contains(Permission::FORM) {
            Changes::FormsAndSigning
        } else if p.contains(Permission::ASSEMBLE) {
            Changes::PageAssembly
        } else {
            Changes::None
        };
        Restrictions {
            printing,
            changes,
            copying: p.contains(Permission::COPY),
        }
    }
}

/// Restrictions and the password that lifts them.
#[derive(Clone, PartialEq, Eq)]
pub struct Restrict {
    /// The permissions (owner) password.
    pub password: String,
    pub restrictions: Restrictions,
}

/// The protection a file is to be written with: a password to open it, restrictions, or
/// both.
#[derive(Clone, PartialEq, Eq)]
pub struct ProtectionSettings {
    /// The password to open the file (the user password).
    pub open_password: Option<String>,
    pub restrict: Option<Restrict>,
}

// Passwords stay out of logs and panic messages.
impl fmt::Debug for ProtectionSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProtectionSettings")
            .field("open_password", &self.open_password.as_ref().map(|_| "…"))
            .field(
                "restrictions",
                &self.restrict.as_ref().map(|r| r.restrictions),
            )
            .finish()
    }
}

/// The longest password, in bytes of UTF-8: revision 6 uses the first 127, and MuPDF's
/// buffers hold 127 and a NUL.
pub const MAX_PASSWORD_BYTES: usize = 127;

impl ProtectionSettings {
    /// Checks what MuPDF would otherwise take badly (the crate panics on a password longer
    /// than its buffer, and a NUL would end one early) and what would protect nothing.
    pub fn validate(&self) -> Result<()> {
        let bad = |why: &str| Err(Error::InvalidArgument(why.into()));
        if self.open_password.is_none() && self.restrict.is_none() {
            return bad("a protection needs a password to open the file or restrictions");
        }
        let passwords = self
            .open_password
            .iter()
            .chain(self.restrict.iter().map(|r| &r.password));
        for password in passwords {
            if password.is_empty() {
                return bad("a password cannot be empty");
            }
            if password.len() > MAX_PASSWORD_BYTES {
                return bad("a password can be up to 127 bytes long");
            }
            if password.contains('\0') {
                return bad("a password cannot contain a NUL character");
            }
        }
        if let (Some(open), Some(restrict)) = (&self.open_password, &self.restrict)
            && *open == restrict.password
        {
            return bad("the permissions password must differ from the password to open");
        }
        Ok(())
    }

    /// What the file allows to whoever opens it without the permissions password.
    pub fn permissions(&self) -> Permission {
        self.restrict
            .as_ref()
            .map_or(Restrictions::NONE, |r| r.restrictions)
            .permissions()
    }

    /// The password the document is reopened with once this is saved: the permissions
    /// password, so whoever set the restrictions is not refused what they restrict, or
    /// else the password to open it.
    pub fn reopen_password(&self) -> Option<&str> {
        self.restrict
            .as_ref()
            .map(|r| r.password.as_str())
            .or(self.open_password.as_deref())
    }

    /// Sets `options` to write this protection. A file with no restrictions gets a random
    /// permissions password nobody is shown: under revision 6 an empty one would open the
    /// file as its owner, around the password to open it. Call [`Self::validate`] first.
    pub(crate) fn apply(&self, options: &mut PdfWriteOptions) {
        let owner = match &self.restrict {
            Some(r) => r.password.clone(),
            None => random_password(),
        };
        options
            .set_encryption(Encryption::Aes256)
            .set_permissions(self.permissions())
            .set_user_password(self.open_password.as_deref().unwrap_or(""))
            .set_owner_password(&owner);
    }
}

/// 64 hexadecimal digits from two random UUIDs: 244 random bits.
fn random_password() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// The permission bits that have a meaning (`Permission::all()`); `/P` has the others set.
const NAMED: u32 = 0xf3c;

/// The cipher a file is encrypted with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cipher {
    Aes,
    Rc4,
    Unknown,
}

/// Each permission a file sets, as its `/P` has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowed {
    pub print: bool,
    pub print_high_resolution: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub assemble: bool,
}

impl Allowed {
    fn from_permissions(p: Permission) -> Allowed {
        Allowed {
            print: p.contains(Permission::PRINT),
            print_high_resolution: p.contains(Permission::PRINT | Permission::PRINT_HQ),
            modify: p.contains(Permission::MODIFY),
            copy: p.contains(Permission::COPY),
            annotate: p.contains(Permission::ANNOTATE),
            fill_forms: p.contains(Permission::FORM),
            assemble: p.contains(Permission::ASSEMBLE),
        }
    }
}

/// The protection a file has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtectionInfo {
    /// It needs a password to open.
    pub open_password: bool,
    /// It restricts something to whoever opens it without its permissions password.
    pub restricted: bool,
    /// What it allows so, permission by permission.
    pub allowed: Allowed,
    /// The same as the choices of the Protect dialog, nearest to what it sets.
    pub restrictions: Restrictions,
    pub cipher: Cipher,
    pub key_bits: u32,
    /// It was opened with its permissions password: nothing it restricts applies.
    pub unrestricted: bool,
}

/// The protection `doc` has, `None` if it has none. `opened_with` is the password `doc`
/// was opened with: telling whether the file needs a password tries the empty one, which
/// MuPDF's authentication then holds, so `doc` is authenticated with `opened_with` again
/// afterwards.
pub fn read(doc: &mut PdfDocument, opened_with: Option<&str>) -> Result<Option<ProtectionInfo>> {
    let Some(crypt) = crypt_info(doc)? else {
        return Ok(None);
    };
    let unrestricted = doc.permissions().contains(Permission::all());
    let open_password = doc.needs_password()?;
    doc.authenticate(opened_with.unwrap_or(""))?;
    // `/P` is a signed 32-bit number with the reserved bits set; only the named ones count.
    let own = || Permission::from_bits_truncate(crypt.permissions as u32 & NAMED);
    let restricted = !own().contains(Permission::all() - Permission::ACCESSIBILITY);
    let cipher = match crypt.method.as_str() {
        "AES" => Cipher::Aes,
        "RC4" => Cipher::Rc4,
        _ => Cipher::Unknown,
    };
    Ok(Some(ProtectionInfo {
        open_password,
        restricted,
        allowed: Allowed::from_permissions(own()),
        restrictions: Restrictions::from_permissions(own()),
        cipher,
        key_bits: u32::try_from(crypt.length).unwrap_or(0),
        unrestricted: unrestricted && restricted,
    }))
}

/// A file picked for Protect, once opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileProtection {
    /// The protection it has now, if any.
    pub protection: Option<ProtectionInfo>,
    /// At least one signature field holds a signature, which a file written whole no
    /// longer matches.
    pub signed: bool,
    /// The PDF/A level it claims, which protection takes away (PDF/A forbids encryption).
    pub pdfa: Option<PdfA>,
}

/// Opens `path` with `password` and says how it is protected and what protecting it
/// changes. Without a password, a file that needs one gives [`Error::PasswordRequired`];
/// a password that does not open it, [`Error::WrongPassword`].
pub fn inspect(path: &Path, password: Option<&str>) -> Result<FileProtection> {
    let mut doc = crate::unprotect::load(path, password)?;
    Ok(FileProtection {
        protection: read(&mut doc, password)?,
        signed: docinfo::is_signed(&doc)?,
        pdfa: docinfo::read_pdfa(&doc),
    })
}

/// Writes `path`, opened with `password`, to `target` with `settings` in place of the
/// protection it had. `target` may be `path` itself: it is replaced in one rename once the
/// new file is written and checked. Returns the PDF/A level the file no longer claims.
pub fn protect_file(
    path: &Path,
    password: Option<&str>,
    settings: &ProtectionSettings,
    target: &Path,
) -> Result<Option<PdfA>> {
    settings.validate()?;
    let mut doc = crate::unprotect::load(path, password)?;
    let removed = docinfo::remove_pdfa_claim(&mut doc)?;
    save_atomic_encrypted(
        &doc,
        SaveKind::Full,
        None,
        target,
        Encrypt::Set(settings),
        &mut || Ok(()),
    )?;
    Ok(removed)
}

/// Checks that the file at `path` opens as `settings` say: with the password to open it
/// and with no other, restricting what they restrict without the permissions password and
/// nothing with it. Before a protected file replaces anything (ADR 0026).
pub(crate) fn verify(path: &Path, settings: &ProtectionSettings) -> Result<()> {
    let fail = |why: &str| {
        Err(Error::InvalidArgument(format!(
            "the protected file did not open as it should: {why}"
        )))
    };
    let mut doc = open_pdf_shared(path)?;
    match crypt_info(&doc)? {
        Some(crypt) if crypt.revision == 6 && crypt.method == "AES" => {}
        _ => return fail("it is not encrypted with AES-256"),
    }
    match &settings.open_password {
        Some(open) => {
            if !doc.needs_password()? {
                return fail("it opens without its password");
            }
            if !doc.authenticate(open)? {
                return fail("its password does not open it");
            }
        }
        None => {
            if doc.needs_password()? {
                return fail("it needs a password to open");
            }
        }
    }
    if doc.permissions().bits() & NAMED != settings.permissions().bits() & NAMED {
        return fail("its restrictions are not the ones chosen");
    }
    if let Some(restrict) = &settings.restrict
        && (!doc.authenticate(&restrict.password)?
            || !doc.permissions().contains(Permission::all()))
    {
        return fail("its permissions password does not lift its restrictions");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(open: Option<&str>, restrict: Option<&str>) -> ProtectionSettings {
        ProtectionSettings {
            open_password: open.map(str::to_owned),
            restrict: restrict.map(|password| Restrict {
                password: password.to_owned(),
                restrictions: Restrictions {
                    printing: Printing::LowResolution,
                    changes: Changes::None,
                    copying: false,
                },
            }),
        }
    }

    #[test]
    fn each_choice_sets_the_bits_acrobat_sets() {
        let bits = |printing, changes, copying| {
            Restrictions {
                printing,
                changes,
                copying,
            }
            .permissions()
            .bits()
                & !Permission::ACCESSIBILITY.bits()
        };
        use Changes as C;
        use Printing as P;
        assert_eq!(
            bits(P::NotAllowed, C::None, false),
            Permission::empty().bits()
        );
        assert_eq!(
            bits(P::LowResolution, C::None, false),
            Permission::PRINT.bits()
        );
        assert_eq!(
            bits(P::HighResolution, C::None, false),
            (Permission::PRINT | Permission::PRINT_HQ).bits()
        );
        assert_eq!(
            bits(P::NotAllowed, C::PageAssembly, false),
            Permission::ASSEMBLE.bits()
        );
        assert_eq!(
            bits(P::NotAllowed, C::FormsAndSigning, false),
            Permission::FORM.bits()
        );
        assert_eq!(
            bits(P::NotAllowed, C::CommentsFormsAndSigning, false),
            (Permission::ANNOTATE | Permission::FORM).bits()
        );
        assert_eq!(
            bits(P::NotAllowed, C::AnyExceptExtracting, true),
            (Permission::MODIFY
                | Permission::ANNOTATE
                | Permission::FORM
                | Permission::ASSEMBLE
                | Permission::COPY)
                .bits()
        );
        assert_eq!(
            Restrictions::NONE.permissions().bits(),
            Permission::all().bits()
        );
    }

    #[test]
    fn every_choice_reads_back_as_itself() {
        use Changes as C;
        use Printing as P;
        for printing in [P::NotAllowed, P::LowResolution, P::HighResolution] {
            for changes in [
                C::None,
                C::PageAssembly,
                C::FormsAndSigning,
                C::CommentsFormsAndSigning,
                C::AnyExceptExtracting,
            ] {
                for copying in [false, true] {
                    let r = Restrictions {
                        printing,
                        changes,
                        copying,
                    };
                    assert_eq!(Restrictions::from_permissions(r.permissions()), r);
                }
            }
        }
    }

    #[test]
    fn validation_refuses_what_would_break_or_protect_nothing() {
        assert!(settings(Some("a"), None).validate().is_ok());
        assert!(settings(None, Some("b")).validate().is_ok());
        assert!(settings(Some("a"), Some("b")).validate().is_ok());
        assert!(settings(None, None).validate().is_err());
        assert!(settings(Some(""), None).validate().is_err());
        assert!(settings(Some("same"), Some("same")).validate().is_err());
        assert!(settings(Some("a\0b"), None).validate().is_err());
        let longest = "é".repeat(63) + "x"; // 127 bytes
        assert!(settings(Some(&longest), None).validate().is_ok());
        let too_long = "é".repeat(64); // 128 bytes
        assert!(settings(Some(&too_long), None).validate().is_err());
    }

    #[test]
    fn no_restrictions_get_a_random_permissions_password() {
        let mut options = PdfWriteOptions::default();
        settings(Some("open"), None).apply(&mut options);
        assert_eq!(options.user_password(), "open");
        assert_eq!(options.owner_password().len(), 64);
        let mut again = PdfWriteOptions::default();
        settings(Some("open"), None).apply(&mut again);
        assert_ne!(options.owner_password(), again.owner_password());
    }

    #[test]
    fn debug_leaves_the_passwords_out() {
        let shown = format!("{:?}", settings(Some("secret1"), Some("secret2")));
        assert!(!shown.contains("secret"), "{shown}");
    }
}
