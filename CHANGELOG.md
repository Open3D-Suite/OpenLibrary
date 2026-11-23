# Changelog

What changed for users in each version of OpenLibrary, newest first.

## In development

- **First version.** Starts the Rust rewrite of OpenLibrary from the Open3D template.
- **Browse your models.** The models tab shows every model of its folders with its preview, or, when it has
  none, the viewport capture 3ds Max saved in the `.max`, with the Corona, V-Ray and FStorm versions it has.
  The folder tree, the search box and the panel of the selected model (its versions, its missing external
  files, *Open folder*) work as in the Python version. The library is read once and then only the folders
  that changed; while it reads, the status bar counts what it has seen.
- **Your settings come along.** The first time, the tabs and folders of the Python version are brought over
  from its settings; that version's own files are not touched.
- In English and in Spanish, as the other Open3D apps.
- **Drag a model into 3ds Max.** Drop a card on a 3ds Max window to merge it and place it with the mouse (it
  rests on surfaces; the wheel turns it, a right click or Esc ends); with Shift it comes in as a Corona or
  V-Ray proxy, with Ctrl in its original place. The same three choices are in the card's right-click menu.
  The version of the scene's renderer is used, and the status bar says when there is none.
- **The bridge in 3ds Max.** When OpenLibrary opens it offers to install its bridge in every 3ds Max 2025 or
  later that lacks it, and to update one that is old. It lives beside the Python version's bridge, which
  keeps working. It obeys only OpenLibrary: every request carries a token, kept encrypted for your Windows
  user.
- **Catalogue a download.** *Catalogue…* opens a zip, rar, 7z (in parts and with a password too) or a folder, proposes
  which files are the `.max`, the preview and the textures, and saves them in a folder of the library under one
  name. Drag files between the lists, drop an image from a folder as the preview, or paste one copied in the browser.
  If asked, a 3ds Max in the background makes the Corona version of a V-Ray `.max` with the Corona Converter, and
  leaves everything in layer 0.
- **Previews and model data in the background.** A 3ds Max in the background makes the preview of a model, or of all
  those without one, and reads faces, vertices, displacement and animation, without touching the scene you have open.
  A file that leaves it stuck is skipped.
- **Missing files.** The panel of a model opens the list of what it does not find: relink it by hand, search for it in
  a folder, use what is in the model's folder, or remove it, and save it in the `.max` without opening it.
