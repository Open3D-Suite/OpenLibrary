-- SPDX-License-Identifier: GPL-3.0-or-later
/*
    OpenLibrary - the jobs of a 3ds Max in the background (3dsmaxbatch). It never travels into the person's
    3ds Max: the app writes it beside the job, after olib_core.ms, and runs both.

    Every job reads jobsFile (UTF-8, one line per file, fields between tabs) and adds to logFile one line per
    line it has done: "ok <tab> number <tab> fields" or "fail <tab> number <tab> reason", numbers from 1.

    olib_batchPreviews jobsFile logFile size margin elev azim   file | preview to write
    olib_batchStats jobsFile logFile                            file
    olib_batchPaths jobsFile logFile                            file | stored path | new path | stored | new...
    olib_batchPrepare jobsFile logFile                          file | clean layers (1/0) | Corona file to make
*/

fn olib_jobLines jobsFile =
(
    local jobs = #()
    -- files of a download come from anybody: their scripts must not be able to do harm
    if not (SceneScriptSecurityManager.IsSafeSceneScriptExecutionEnabled #Current) do
    (
        format "OpenLibrary: Safe Scene Script Execution is off: nothing is opened\n"
        return jobs
    )
    local jf = openFile jobsFile mode:"r"
    while not (eof jf) do
    (
        local line = readLine jf
        if line != "" do append jobs (filterString line "\t" splitEmptyTokens:true)
    )
    close jf
    jobs
)

fn olib_jobLog logFile ok n text =
(
    local lf = openFile logFile mode:"a" encoding:#utf8
    if lf != undefined do
    (
        format "%\t%\t%\n" (if ok then "ok" else "fail") n text to:lf
        close lf
    )
)

-- A message on one line and without tabs, for the log.
fn olib_oneLine text = substituteString (substituteString (substituteString (text as string) "\r" " ") "\n" " ") "\t" " "

-- What the app shows about a scene, between tabs: faces, vertices, geometry objects, displacement (1/0),
-- animation (1/0), first and last frame.
fn olib_sceneStats =
(
    local polys = 0
    local verts = 0
    local nGeo = 0
    for o in geometry where not (isKindOf o TargetObject) do
    (
        nGeo += 1
        try ( local c = getPolygonCount o; polys += c[1]; verts += c[2] ) catch ()
    )
    -- displacement: a modifier, or a material whose displacement map is on
    local disp = false
    for o in objects while not disp do
        for m in o.modifiers while not disp do
        (
            local cn = toLower ((classOf m) as string)
            if cn == "displace" or matchPattern cn pattern:"*displacement*" do disp = m.enabled
        )
    local checks = #(#(CoronaPhysicalMtl, #displacementTexmap, #displacementTexmapOn),
                     #(CoronaLegacyMtl, #texmapDisplace, #texmapOnDisplacement),
                     #(CoronaMtl, #texmapDisplace, #texmapOnDisplacement),
                     #(VRayMtl, #texmap_displacement, #texmap_displacement_on),
                     #(PhysicalMaterial, #displacement_map, #displacement_map_on))
    for c in checks while not disp do
    (
        local cls = c[1]
        if cls != undefined do
            try
            (
                for m in (getClassInstances cls) while not disp do
                    try ( if (getProperty m c[2]) != undefined and (getProperty m c[3]) == true do disp = true ) catch ()
            )
            catch ()
    )
    local anim = false
    for o in objects while not anim do try ( if o.isAnimated do anim = true ) catch ()
    local s = 0
    local e = 0
    try ( s = animationRange.start.frame as integer; e = animationRange.end.frame as integer ) catch ()
    (polys as string) + "\t" + (verts as string) + "\t" + (nGeo as string) + "\t" + \
        (if disp then "1" else "0") + "\t" + (if anim then "1" else "0") + "\t" + (s as string) + "\t" + (e as string)
)

fn olib_batchStats jobsFile logFile =
(
    local n = 0
    for j in (olib_jobLines jobsFile) do
    (
        n += 1
        local st = undefined
        try ( if (loadMaxFile j[1] quiet:true useFileUnits:true) do st = olib_sceneStats() )
        catch ( format "OpenLibrary: % - %\n" j[1] (getCurrentException()) )
        if st != undefined then olib_jobLog logFile true n st else olib_jobLog logFile false n "open"
    )
    n
)

fn olib_batchPreviews jobsFile logFile size margin elev azim =
(
    local n = 0
    for j in (olib_jobLines jobsFile) do
    (
        n += 1
        local ok = false
        local st = ""
        local msg = ""
        try
        (
            if not (loadMaxFile j[1] quiet:true useFileUnits:true) then msg = "open"
            else
            (
                try ( st = olib_sceneStats() ) catch ()
                local nodes = for o in objects where not o.isHidden and (superclassof o) != light \
                    and (superclassof o) != camera collect o
                -- framed on the geometry alone: helpers and splines make the box larger
                local geo = for o in nodes where (superclassof o) == GeometryClass collect o
                if geo.count > 0 do nodes = geo
                if nodes.count > 0 do
                (
                    -- without lights of its own the model would come out black: an even light all around
                    useEnvironmentMap = false
                    backgroundColor = color 228 228 228
                    select nodes
                    ok = olib_makePreview j[2] size "render" margin elev azim false
                )
            )
        )
        catch ( msg = olib_oneLine (getCurrentException()) )
        olib_jobLog logFile ok n (if ok then st else msg)
    )
    n
)

-- Changes or empties the paths of external files inside a .max without opening it: only its table of
-- external files is written again. An empty new path removes the reference.
fn olib_batchPaths jobsFile logFile =
(
    local n = 0
    for j in (olib_jobLines jobsFile) do
    (
        n += 1
        local changed = 0
        local ok = false
        local msg = ""
        try
        (
            local meta = getMAXFileAssetMetadata j[1]
            if meta == undefined then msg = "open"
            else
            (
                for a in meta do
                (
                    local stored = a.filename as string
                    local done = false
                    for i = 2 to j.count - 1 by 2 while not done where (stricmp stored j[i]) == 0 do
                    (
                        a.filename = j[i + 1]
                        changed += 1
                        done = true
                    )
                )
                ok = if changed > 0 then (setMAXFileAssetMetadata j[1] meta) == true else true
                if not ok do msg = "save"
            )
        )
        catch ( ok = false; msg = olib_oneLine (getCurrentException()) )
        olib_jobLog logFile ok n (if ok then changed as string else msg)
    )
    n
)

-- Every object to layer 0, and the other layers out. Returns how many layers are left.
fn olib_cleanLayers =
(
    local l0 = LayerManager.getLayer 0
    l0.current = true
    for o in objects do l0.addNode o
    local again = true
    while again do      -- a layer with child layers goes only after them
    (
        again = false
        for i = LayerManager.count - 1 to 1 by -1 do
        (
            local l = LayerManager.getLayer i
            if l != undefined and (LayerManager.deleteLayerByName l.name) do again = true
        )
    )
    LayerManager.count
)

global CoronaConverter          -- made by coronaConverter.ms: declared first so that the functions see them
global CoronaProxyConverter
global CoronaConverterSuppressGui

-- The Corona Converter that comes with Corona (Scripts\coronaConverter.ms, where Corona for this 3ds Max was
-- installed), loaded without its window.
fn olib_loadConverter =
(
    if CoronaConverter != undefined do return true
    local yr = (1998 + (maxVersion())[1] / 1000) as string
    local pf = systemTools.getEnvVariable "ProgramFiles"
    local dirs = #(pf + "\\Chaos\\Corona\\Corona Renderer for 3ds Max\\" + yr + "\\Scripts\\",
                   pf + "\\Corona\\Corona Renderer for 3ds Max\\" + yr + "\\Scripts\\")
    for i = 1 to pluginManager.pluginDllCount do           -- Corona installed somewhere else
    (
        local dll = pluginManager.pluginDllFullPath i
        if dll != undefined and (matchPattern (filenameFromPath dll) pattern:"coronamax20*.dl?") do
            append dirs ((getFilenamePath dll) + "Scripts\\")
    )
    local script = undefined
    for d in dirs while script == undefined where doesFileExist (d + "coronaConverter.ms") do
        script = d + "coronaConverter.ms"
    if script == undefined do return false
    CoronaConverterSuppressGui = true
    fileIn script
    CoronaConverter != undefined
)

-- VRayDisplacementMod -> CoronaDisplacementMod (the Corona Converter's button cannot be called from outside):
-- the same map, minimum and maximum level = shift and shift + amount, and the water level on that scale.
fn olib_vrayDisplToCorona =
(
    local n = 0
    for m in (getClassInstances VRayDisplacementMod) where classof m == VRayDisplacementMod do
    (
        local c = CoronaDisplacementMod()
        c.mapSource = 1
        c.texmap = m.texmap
        c.levelMin = m.shift
        c.levelMax = m.shift + m.amount
        c.useWaterLevel = m.waterLevelOn
        if m.amount != 0 do c.waterLevel = (m.waterLevelValue - m.shift) / m.amount
        replaceInstances m c
        n += 1
    )
    n
)

-- What the Corona Converter does by hand: Convert (materials, maps, lights, VRayHDRI, V-Ray proxies), then
-- Disable CoronaRoundEdges legacy, Fix CoronaNormal gamma, VRayDisplacementMod -> CoronaDisplacementMod and
-- Bitmap -> CoronaBitmap.
fn olib_toCorona =
(
    local cc = CoronaConverter
    local st = cc.converterSettings
    st.silent = true
    st.showWarnings = false
    st.switchRenderEngine = true
    st.VrayHDRIToBitmaptexture = true
    st.translucencyEnablesThinShell = true
    st.bitmapTextureToCoronaBitmap = false
    st.addCoronaPrefix = false
    with animate off
    (
        cc.convertSceneMtlsMaps ErrorReport:false
        cc.convertSceneLights ErrorReport:false
        if vray != undefined and (getClassInstances VRayProxy).count > 0 do
            try ( CoronaProxyConverter.convertProxies() )
            catch ( format "OpenLibrary: V-Ray proxies not converted: %\n" (getCurrentException()) )
        cc.converterTools.disableCoronaRoundEdgesMapsLegacyMode()
        cc.converterTools.fixCoronaNormalGamma()
        if vray != undefined do olib_vrayDisplToCorona()
        cc.converterTools.convertBitmapToCoronaBitmap()
    )
    if substring ((classof renderers.current) as string) 1 6 != "Corona" do renderers.current = Corona()
    true
)

-- Saves in the version of the original when it can (3ds Max saves at most three versions back), so that an
-- older 3ds Max still opens it.
fn olib_saveLike f srcVersion =
(
    local cur = 1998 + (maxVersion())[1] / 1000
    local yr = amax srcVersion (cur - 3)
    if yr < cur then saveMaxFile f saveAsVersion:yr quiet:true useNewFile:false
    else saveMaxFile f quiet:true useNewFile:false
)

-- The files just catalogued: layers cleaned and the Corona version made. Reasons the app has words for:
-- open, save, converter, vray.
fn olib_batchPrepare jobsFile logFile =
(
    local n = 0
    for j in (olib_jobLines jobsFile) do
    (
        n += 1
        local ok = false
        local msg = ""
        try
        (
            local ver = getMaxFileVersionData j[1]
            local srcYear = if ver != undefined and ver[1] != undefined then 1998 + ver[1] / 1000 else 0
            if not (loadMaxFile j[1] quiet:true useFileUnits:true) then msg = "open"
            else
            (
                ok = true
                if j[2] == "1" do
                (
                    olib_cleanLayers()
                    ok = olib_saveLike j[1] srcYear
                    if not ok do msg = "save"
                )
                if ok and j.count > 2 and j[3] != "" do
                (
                    ok = false
                    if not (olib_loadConverter()) then msg = "converter"
                    else if vray == undefined then msg = "vray"
                    else
                    (
                        olib_toCorona()
                        ok = olib_saveLike j[3] srcYear
                        if not ok do msg = "save"
                    )
                )
            )
        )
        catch ( ok = false; msg = olib_oneLine (getCurrentException()) )
        olib_jobLog logFile ok n msg
        resetMaxFile #noPrompt
    )
    n
)
