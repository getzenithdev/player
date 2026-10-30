### This file defines what the application does and how it does it.

The complete development plan is documented in [roadmap.md](roadmap.md), with 16 phases, dependencies, deliverables, acceptance criteria, and release scope. The supplied phase 3 is phase 1 in this repository. Features and platforms below are targets pending implementation and validation.

### Scope
features that will be available
*Playback of local music.*
*Reproduction from authorized third-party providers.*
*Integration with cloud storage services*
*Song search.*
*Music library.*
*Artist and album management.*
*Playlist management.*
*Synchronized and unsynchronized lyrics.*
*Static covers.*
*Animated covers.*
*Favorites.*
*Configuration.*
*Automatic updates.*


### Platforms

| Plataform         | Architecture|
| :---              | :---        |
| macOS Monterey 12 | Intel / ARM |
| macOS Ventura 13  | Intel / ARM |
| macOS Sonoma 14   | Intel / ARM |
| macOS Sequoia 15  | Intel / ARM |
| macOS Tahoe 26    | Intel / ARM |
| Windows 10        | x64         |
| Windows 11        | x64 / ARM64 |
| Linux             | x64         |


### Project rules

*Do not use databases.*
*Do not use your own servers.*
*Do not require developer accounts.*
*Store the configuration locally.*
*Do not duplicate music files unnecessarily.*
*External providers must be independent.*
*The application must continue to function without an Internet connection for content that has been previously downloaded.*
*External providers must respect their APIs and licenses.*
*No external source should be indispensable for launching the application.*

