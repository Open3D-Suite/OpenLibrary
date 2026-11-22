-- SPDX-License-Identifier: GPL-3.0-or-later
/*
    OpenLibrary bridge - the MAXScript core.
    Everything that touches the scene lives here. The bridge (Python) calls these global functions with
    positional arguments. Every global starts with olib_.

    olib_sceneEngine()                                              -> "corona" | "vray" | "fstorm" | "other"
    olib_merge f name group layerMode objDups mtlDups lights cameras  -> #(nodes)
    olib_place nodes onSurface multi                                -> #(placed nodes)
    olib_mergeProxy f name proxyDir mode engine layerMode mtlDups display -> #(nodes)
    olib_makePreview outFile size method margin elev azim fromView  -> true | false
    olib_sceneFiles()                                               -> #(paths of the scene's assets)
    olib_setFileDir oldPath newDir                                  -> true | false
*/

global olib_placeData
struct olib_PlaceStruct (nodes = #(), roots = #(), anchor = [0,0,0], onSurface = true, placed = false, clicked = false, midAbort = false, midWas = false)

-- Whether the mouse wheel (the middle button) is pressed right now.
global olib_ctlClass, olib_btnMiddle
fn olib_midDown =
(
    try
    (
        if olib_ctlClass == undefined do
        (
            olib_ctlClass = dotNetClass "System.Windows.Forms.Control"
            olib_btnMiddle = (dotNetClass "System.Windows.Forms.MouseButtons").Middle
        )
        dotNet.compareEnums olib_ctlClass.MouseButtons olib_btnMiddle
    )
    catch ( false )
)

-- ---------------------------------------------------------------------------
-- Helpers
-- ---------------------------------------------------------------------------
fn olib_sceneEngine =
(
    local n = toLower ((classof renderers.current) as string)
    if matchPattern n pattern:"*corona*" then "corona"
    else if (matchPattern n pattern:"*v_ray*") or (matchPattern n pattern:"*vray*") then "vray"
    else if matchPattern n pattern:"*fstorm*" then "fstorm"
    else "other"
)

-- The bridge learns of changes through callbacks, not by asking every second. The Python text is fixed:
-- nothing read from a file or a request goes into it.
fn olib_stateChanged =
(
    try ( python.execute "import olib_bridge.server as _olib_server\n_olib_server.refresh_state()" ) catch ()
)

fn olib_watchState enable =
(
    callbacks.removeScripts id:#Open3DOpenLibraryState
    if enable do
        for ev in #(#filePostOpen, #filePostSave, #systemPostNew, #systemPostReset, #postRendererChange, \
                    #selectionSetChanged) do
            callbacks.addScript ev "olib_stateChanged()" id:#Open3DOpenLibraryState
    ok
)

fn olib_safeName s =
(
    local bad = "\\/:*?\"<>| "
    local out = ""
    for i = 1 to s.count do
    (
        local c = s[i]
        out += (if (findString bad c) != undefined then "_" else c)
    )
    out
)

fn olib_roots nodes =
(
    for n in nodes where isValidNode n and (n.parent == undefined or (findItem nodes n.parent) == 0) collect n
)

fn olib_bbox nodes =
(
    local mn = [1e30, 1e30, 1e30]
    local mx = [-1e30, -1e30, -1e30]
    local any = false
    for n in nodes where isValidNode n and not (isGroupHead n) do
    (
        local a = n.min
        local b = n.max
        if a.x > b.x then continue   -- objects without geometry
        mn = [amin mn.x a.x, amin mn.y a.y, amin mn.z a.z]
        mx = [amax mx.x b.x, amax mx.y b.y, amax mx.z b.z]
        any = true
    )
    if not any then
    (
        mn = [0,0,0]
        mx = [0,0,0]
        for n in nodes where isValidNode n do ( mn = n.pos; mx = n.pos )
    )
    #(mn, mx)
)

fn olib_layerNames =
(
    for i = 0 to LayerManager.count - 1 collect (LayerManager.getLayer i).name
)

-- Deletes the empty layers a merge brought in.
fn olib_cleanNewLayers before =
(
    local names = olib_layerNames()
    for n in names where (findItem before n) == 0 do
        try (LayerManager.deleteLayerByName n) catch ()
)

fn olib_assignLayer nodes layerMode assetName =
(
    local lay = undefined
    if layerMode == "asset" then
    (
        lay = LayerManager.getLayerFromName assetName
        if lay == undefined then lay = LayerManager.newLayerFromName assetName
    )
    else if layerMode == "current" then lay = LayerManager.current
    if lay != undefined then
        for n in nodes where isValidNode n do lay.addNode n
)

-- ---------------------------------------------------------------------------
-- Merge
-- ---------------------------------------------------------------------------
fn olib_merge f assetName doGroup layerMode objDups mtlDups lights cameras =
(
    if not (doesFileExist f) then return #()

    local objOpt = case objDups of
    (
        "merge":   #mergeDups
        "skip":    #skipDups
        "replace": #deleteOldDups
        "ask":     #promptDups
        default:   #autoRenameDups
    )
    local mtlOpt = case mtlDups of
    (
        "file":    #useMergedMtlDups
        "rename":  #renameMtlDups
        "ask":     #promptMtlDups
        default:   #useSceneMtlDups
    )

    local layersBefore = olib_layerNames()
    clearSelection()
    local ok = mergeMAXFile f objOpt mtlOpt #neverReparent #select #noRedraw quiet:true
    if not ok then return #()
    local nodes = selection as array

    -- Lights and cameras
    if (not lights) or (not cameras) then
    (
        local del = #()
        for n in nodes where isValidNode n do
        (
            local sc = superclassof n
            if ((not lights) and sc == light) or ((not cameras) and sc == camera) then
            (
                append del n
                if (isProperty n #target) and n.target != undefined then append del n.target
            )
        )
        for d in del where isValidNode d do delete d
        nodes = for n in nodes where isValidNode n collect n
    )

    olib_assignLayer nodes layerMode assetName

    if doGroup and nodes.count > 0 then
    (
        local tops = olib_roots nodes
        if tops.count > 0 then
        (
            local g = group tops name:(uniqueName (assetName + "_"))
            append nodes g
            olib_assignLayer #(g) layerMode assetName
        )
    )

    if layerMode != "file" then olib_cleanNewLayers layersBefore
    select nodes
    nodes
)

-- ---------------------------------------------------------------------------
-- Merge and place: the model follows the mouse and rests on surfaces.
--   Left click         = drop it here (and, placing several, go on with an instance)
--   Wheel (press)      = turn 90 degrees around Z
--   Right button / Esc = end
--   Ctrl               = ignore surfaces, grid only
-- ---------------------------------------------------------------------------
fn olib_surfacePoint =
(
    local best = undefined
    try
    (
        local r = mapScreenToWorldRay mouse.pos
        local hits = intersectRayScene r
        local bestD = 1e30
        for h in hits do
        (
            local nd = h[1]
            if (findItem olib_placeData.nodes nd) == 0 and not nd.isHidden then
            (
                local d = distance r.pos h[2].pos
                if d < bestD then ( bestD = d; best = h[2].pos )
            )
        )
    )
    catch ()
    best
)

fn olib_placeAt ir gridOnly =
(
    local pos = undefined
    if olib_placeData.onSurface and not gridOnly then pos = olib_surfacePoint()
    if pos == undefined and ir != undefined then pos = ir.pos
    if pos != undefined then
    (
        local delta = pos - olib_placeData.anchor
        for n in olib_placeData.roots where isValidNode n do n.pos += delta
        olib_placeData.anchor = pos
        olib_placeData.placed = true
        redrawViews()
    )
)

fn olib_rotateCurrent deg =
(
    local rot = eulerAngles 0 0 deg
    about olib_placeData.anchor
    (
        for n in olib_placeData.roots where isValidNode n do rotate n rot
    )
    redrawViews()
)

-- When the wheel goes down (not while it stays down): turn 90 degrees.
fn olib_checkMid =
(
    local down = olib_midDown()
    if down and not olib_placeData.midWas then olib_rotateCurrent 90.0
    olib_placeData.midWas = down
)

fn olib_trackCB msg ir obj faceNum shift ctrl alt =
(
    case msg of
    (
        #freeMove:   ( olib_checkMid(); olib_placeAt ir ctrl; #continue )
        #mouseMove:  ( olib_checkMid(); olib_placeAt ir ctrl; #continue )
        #mousePoint: ( olib_placeAt ir ctrl; olib_placeData.clicked = true; #stop )
        -- the right button (or Esc) interrupts; if it was the wheel, the model turns and placing goes on
        #mouseAbort: ( olib_placeData.midAbort = olib_midDown() or olib_placeData.midWas; #abort )
        default:     #continue
    )
)

fn olib_cloneCurrent =
(
    local nn = #()
    try ( maxOps.cloneNodes olib_placeData.nodes cloneType:#instance newNodes:&nn ) catch ( nn = #() )
    nn
)

fn olib_place nodes onSurface multi =
(
    nodes = for n in nodes where isValidNode n collect n
    if nodes.count == 0 then return #()
    local bb = olib_bbox nodes
    local anchor = [(bb[1].x + bb[2].x) / 2.0, (bb[1].y + bb[2].y) / 2.0, bb[1].z]
    olib_placeData = olib_PlaceStruct nodes:nodes roots:(olib_roots nodes) anchor:anchor onSurface:onSurface
    local placedNodes = #()
    local done = false
    local quickAborts = 0
    local loops = 0
    pushPrompt ("OpenLibrary: click = place" + (if multi then " and go on with another instance" else "") + \
        " | wheel = turn 90 degrees | right button or Esc = end | Ctrl = grid only")
    while not done do
    (
        loops += 1
        olib_placeData.clicked = false
        olib_placeData.midAbort = false
        local t0 = timeStamp()
        try ( mouseTrack trackCallback:olib_trackCB ) catch ()
        -- Interrupted by the wheel: turn and go on. The right button, Esc or anything else ends.
        local midClick = (olib_placeData.midAbort or olib_midDown()) and not keyboard.escPressed
        if olib_placeData.clicked then
        (
            quickAborts = 0
            join placedNodes olib_placeData.nodes
            if not multi then done = true
            else
            (
                local nn = olib_cloneCurrent()
                if nn.count == 0 then done = true
                else
                (
                    olib_placeData.nodes = nn
                    olib_placeData.roots = olib_roots nn
                )
            )
        )
        else if not midClick then done = true
        else
        (
            -- If the callback did not turn it already when the wheel went down, turn it now
            local already = olib_placeData.midWas
            -- Wait for the wheel to come up, so that it does not turn twice
            local w = 0
            while olib_midDown() and w < 100 do ( sleep 0.01; w += 1 )
            olib_placeData.midWas = false
            -- A safety: if mouseTrack returns at once again and again, stop
            if (timeStamp() - t0) < 40 then quickAborts += 1 else quickAborts = 0
            if quickAborts > 3 or loops > 10000 then done = true
            else if not already then olib_rotateCurrent 90.0
        )
    )
    popPrompt()
    if placedNodes.count > 0 then
    (
        -- The copy still floating is deleted
        local floating = for n in olib_placeData.nodes where isValidNode n and (findItem placedNodes n) == 0 collect n
        if floating.count > 0 then delete floating
    )
    else placedNodes = olib_placeData.nodes
    placedNodes = for n in placedNodes where isValidNode n collect n
    select placedNodes
    placedNodes
)

-- ---------------------------------------------------------------------------
-- Proxies (Corona .cgeo / V-Ray .vrmesh)
-- ---------------------------------------------------------------------------
fn olib_proxyExt engine = if engine == "vray" then ".vrmesh" else ".cgeo"

-- Beside each proxy file, the transform of the proxy relative to its source, as 12 numbers.
fn olib_writeRel fname rel =
(
    try
    (
        local fs = createFile (fname + ".olb")
        format "% % % % % % % % % % % %\n" rel.row1.x rel.row1.y rel.row1.z rel.row2.x rel.row2.y rel.row2.z \
            rel.row3.x rel.row3.y rel.row3.z rel.row4.x rel.row4.y rel.row4.z to:fs
        close fs
    )
    catch ()
)

-- Reads it back as numbers only: the file is never run as code. It also reads the files of the Python version,
-- which wrote "(matrix3 [a,b,c] [d,e,f] [g,h,i] [j,k,l])".
fn olib_readRel fname =
(
    local rel = matrix3 1
    local side = fname + ".olb"
    if doesFileExist side then
    (
        try
        (
            local fs = openFile side
            local txt = readLine fs
            close fs
            local nums = for t in (filterString txt " ,[]()\t") where t != "matrix3" collect (t as float)
            if nums.count == 12 and (findItem nums undefined) == 0 then
                rel = matrix3 [nums[1], nums[2], nums[3]] [nums[4], nums[5], nums[6]] \
                    [nums[7], nums[8], nums[9]] [nums[10], nums[11], nums[12]]
        )
        catch ()
    )
    rel
)

fn olib_newProxyFromFile fname engine =
(
    local p = undefined
    if engine == "vray" then
    (
        try ( p = VRayProxy fileName:fname ) catch ( p = undefined )
    )
    else
    (
        try
        (
            p = CProxy()
            try ( p.filename = fname ) catch ( try ( p.cacheFilename = fname ) catch () )
        )
        catch ( p = undefined )
    )
    p
)

-- Exports the node src to the file, and returns the proxy node made from it.
fn olib_exportProxy src fname engine =
(
    local p = undefined
    if engine == "vray" then
    (
        local before = for o in objects where (classof o) == VRayProxy collect o
        try
        (
            vrayMeshExport meshFile:fname nodes:#(src) autoCreateProxies:true exportMultiple:false animation:off
        )
        catch ( format "OpenLibrary: vrayMeshExport failed: %\n" (getCurrentException()) )
        local after = for o in objects where (classof o) == VRayProxy and (findItem before o) == 0 collect o
        if after.count > 0 then p = after[1]
        else
        (
            p = olib_newProxyFromFile fname engine
            if p != undefined then p.transform = src.transform
        )
    )
    else
    (
        try
        (
            p = CProxy()
            CProxy.ProxyFp.fromScene p src fname
            p.transform = src.transform
        )
        catch
        (
            format "OpenLibrary: the CProxy could not be made: %\n" (getCurrentException())
            if p != undefined and isValidNode p then delete p
            p = undefined
        )
    )
    if p != undefined and isValidNode src then olib_writeRel fname (p.transform * (inverse src.transform))
    p
)

-- A temporary mesh in local space, with its pivot at the base of the object.
fn olib_bakeMesh n piv =
(
    local m = snapshotAsMesh n
    if piv == undefined then
        piv = [(n.min.x + n.max.x) / 2.0, (n.min.y + n.max.y) / 2.0, n.min.z]
    if m.numverts > 0 then meshop.moveVert m #{1..m.numverts} (-piv)
    local tmp = Editable_mesh()
    tmp.mesh = m
    tmp.transform = transMatrix piv
    tmp.material = n.material
    tmp.name = n.name
    tmp
)

-- How the proxy shows in the viewport: "bbox" | "preview" | "full".
--   V-Ray (SDK pb2meshproxy.h): 0 box, 1 preview, 2 point, 3 preview faces, 4 full mesh
--   Corona: 0 solid box, 1 wire box, 2 point cloud, 3 full mesh
fn olib_setProxyDisplay p engine mode =
(
    if p == undefined or not (isValidNode p) then return false
    local v
    if engine == "vray" then
    (
        v = case mode of ( "bbox": 0; "full": 4; default: 3 )
        try ( p.display = v; true ) catch ( false )
    )
    else
    (
        v = case mode of ( "bbox": 1; "full": 3; default: 2 )
        try ( p.previzType = v; true ) catch ( false )
    )
)

fn olib_isGeo n =
(
    isValidNode n and (superclassof n) == GeometryClass and (classof n) != TargetObject \
        and (canConvertTo n Editable_Mesh) and not (isGroupHead n)
)

fn olib_mergeProxy f assetName proxyDir mode engine layerMode mtlDups display =
(
    -- A proxy made before (in this project or another): the small .max with the proxies and their
    -- materials comes in, without merging the heavy model again.
    local cacheMax = proxyDir + "\\proxies.max"
    if (doesFileExist (proxyDir + "\\done.olb")) and (doesFileExist cacheMax) then
    (
        local cached = olib_merge cacheMax assetName false layerMode "autorename" mtlDups false false
        if cached.count > 0 then
        (
            for n in cached do olib_setProxyDisplay n engine display
            return cached
        )
    )

    local nodes = olib_merge f assetName false "current" "autorename" mtlDups false false
    if nodes.count == 0 then return #()
    makeDir proxyDir all:true
    local reuse = doesFileExist (proxyDir + "\\done.olb")
    local ext = olib_proxyExt engine
    local geo = for n in nodes where olib_isGeo n collect n
    local result = #()
    local base = olib_safeName assetName

    -- Temporary sources in clean space
    local sources = #()
    if mode == "single" and geo.count > 0 then
    (
        -- One pivot, at the base of the whole
        local bb = olib_bbox geo
        local piv = [(bb[1].x + bb[2].x) / 2.0, (bb[1].y + bb[2].y) / 2.0, bb[1].z]
        local tgt = olib_bakeMesh geo[1] piv
        tgt.name = assetName
        for i = 2 to geo.count do
        (
            local t = olib_bakeMesh geo[i] piv
            try ( meshop.attach tgt t attachMat:#IDToMat condenseMat:true deleteSourceNode:true ) catch ( delete t )
        )
        sources = #(tgt)
    )
    else
    (
        sources = for g in geo collect olib_bakeMesh g undefined
    )

    -- The merged original goes
    for n in nodes where isValidNode n and (isGroupMember n) do setGroupMember n false
    for n in nodes where isValidNode n do n.parent = undefined
    for n in nodes where isValidNode n do delete n

    for i = 1 to sources.count do
    (
        local src = sources[i]
        local fname = proxyDir + "\\" + base + (if sources.count > 1 then ("_" + (formattedPrint i format:"03d")) else "") + ext
        local p = undefined
        if reuse and (doesFileExist fname) then
        (
            p = olib_newProxyFromFile fname engine
            if p != undefined then p.transform = (olib_readRel fname) * src.transform
        )
        if p == undefined then p = olib_exportProxy src fname engine
        if p != undefined then
        (
            p.name = uniqueName (src.name + "_proxy")
            p.material = src.material
            olib_setProxyDisplay p engine display
            append result p
        )
        if isValidNode src then delete src
    )

    if result.count > 0 then
    (
        try ( saveNodes result cacheMax quiet:true ) catch ()
        try
        (
            local fs = createFile (proxyDir + "\\done.olb")
            format "%\n" f to:fs
            close fs
        ) catch ()
    )
    olib_assignLayer result layerMode assetName
    select result
    result
)

-- ---------------------------------------------------------------------------
-- Preview: framed by the bounding box (as good for a chair as for a building), or from the active view.
-- ---------------------------------------------------------------------------
fn olib_fitCamera nodes fovDeg margin elev azim =
(
    local bb = olib_bbox nodes
    local c = (bb[1] + bb[2]) / 2.0
    elev = amax -85.0 (amin 85.0 elev)
    local dir = normalize [sin(azim) * cos(elev), -cos(azim) * cos(elev), sin(elev)]
    local xAx = normalize (cross [0,0,1] dir)
    local yAx = cross dir xAx
    local t = (tan (fovDeg / 2.0)) / margin
    local d = 0.0
    local a = bb[1]
    local b = bb[2]
    for cx in #(a.x, b.x) do for cy in #(a.y, b.y) do for cz in #(a.z, b.z) do
    (
        local v = [cx, cy, cz] - c
        local along = dot v dir
        local need = amax ((abs (dot v xAx)) / t + along) ((abs (dot v yAx)) / t + along)
        if need > d then d = need
    )
    if d < 1.0 then d = 1.0
    local pos = c + dir * d
    Freecamera name:(uniqueName "olib_previewCam") fov:fovDeg transform:(matrix3 xAx yAx dir pos) \
        nearclip:0.1 farclip:(d * 4.0)
)

fn olib_makePreview outFile size method margin elev azim fromView =
(
    local nodes = selection as array
    if nodes.count == 0 and not fromView then return false
    -- the members of selected groups too
    for n in (selection as array) where isGroupHead n do
        for ch in (n.children) where (findItem nodes ch) == 0 do append nodes ch

    local others = for o in objects where not o.isHidden and (findItem nodes o) == 0 \
        and (superclassof o) != light collect o
    local cam = undefined
    local ok = false
    local oldPass = undefined
    with redraw off
    (
        if nodes.count > 0 then hide others
    )
    try
    (
        local bmp = undefined
        if method == "viewport" then
        (
            local oldCam = viewport.getCamera()
            local oldTM = viewport.getTM()
            local oldType = viewport.getType()
            local w = gw.getWinSizeX()
            local h = gw.getWinSizeY()
            if not fromView then
            (
                -- a horizontal field of view that frames the central square crop well
                local vfov = 40.0
                local hfov = 2.0 * (atan ((tan (vfov / 2.0)) * (w as float) / (h as float)))
                cam = olib_fitCamera nodes vfov margin elev azim
                cam.fov = hfov
                viewport.setCamera cam
            )
            completeRedraw()
            local dib = gw.getViewportDib()
            local side = amin dib.width dib.height
            local sq = bitmap side side
            pasteBitmap dib sq (box2 ((dib.width - side) / 2) ((dib.height - side) / 2) side side) [0,0]
            bmp = bitmap size size
            copy sq bmp
            close sq
            close dib
            if not fromView then
            (
                if oldCam != undefined then viewport.setCamera oldCam
                else ( viewport.setType oldType; viewport.setTM oldTM )
            )
        )
        else
        (
            -- a render with the active renderer, with few passes so that it is quick
            if (olib_sceneEngine()) == "corona" then
            (
                try ( oldPass = renderers.current.progressive_passLimit; renderers.current.progressive_passLimit = 12 ) catch ()
            )
            if fromView then
                bmp = render outputSize:[size, size] vfb:false quiet:true
            else
            (
                cam = olib_fitCamera nodes 40.0 margin elev azim
                bmp = render camera:cam outputSize:[size, size] vfb:false quiet:true
            )
        )
        if bmp != undefined then
        (
            makeDir (getFilenamePath outFile) all:true
            bmp.filename = outFile
            save bmp
            close bmp
            ok = true
        )
    )
    catch ( format "OpenLibrary: the preview could not be made: %\n" (getCurrentException()) )

    if oldPass != undefined then try ( renderers.current.progressive_passLimit = oldPass ) catch ()
    if cam != undefined and isValidNode cam then delete cam
    with redraw off ( for o in others where isValidNode o do o.isHidden = false )
    redrawViews()
    ok
)

-- ---------------------------------------------------------------------------
-- Paths of the scene (Asset Tracking)
-- ---------------------------------------------------------------------------
fn olib_sceneFiles =
(
    local fl = #()
    try ( ATSOps.Refresh(); ATSOps.GetFiles &fl ) catch ()
    fl
)

fn olib_setFileDir oldPath newDir =
(
    try
    (
        ATSOps.ClearSelection()
        ATSOps.SelectFiles #(oldPath)
        ATSOps.SetPathOnSelection newDir
        ATSOps.ClearSelection()
        true
    )
    catch ( false )
)

format "OpenLibrary: MAXScript core loaded\n"
