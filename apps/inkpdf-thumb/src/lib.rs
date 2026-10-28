//! The Explorer thumbnail handler for InkPDF: a COM DLL the shell loads to draw a PDF's first page
//! as the file's icon, the way Acrobat does. The installer registers it for InkPDF's PDF type, so
//! it applies when InkPDF is the default PDF app.
//!
//! The shell calls it with the file's bytes as a stream and asks for a bitmap of a given size. The
//! DLL does not read PDFs itself: it runs `inkpdf.exe --thumbnail SIZE` from the same folder, gives
//! it the file and reads the picture back (see `client`). A damaged or hostile PDF can then only
//! hurt that helper, which is killed if it takes too long, and never the program showing the
//! thumbnails.

#[cfg(windows)]
mod client;
#[cfg(windows)]
mod shell;
