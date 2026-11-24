-- SPDX-License-Identifier: GPL-3.0-or-later
/*
    OpenLibrary bridge - materials, textures and interiors. Loaded after olib_core.ms; every global starts
    with olib_.

    olib_pbrMaterial name engine spec mapping wCm hCm useDisp       -> material
    olib_putMaterials mats target uvwMode xCm yCm zCm               -> "where|count|name"
    olib_slateShow mats                                             -> how many are new in Slate
    olib_interiorMaterial name engine osl exr preview size depth    -> material
    olib_putInterior mat target wCm hCm                             -> "where|count|name"
*/

-- (from Python come doubles: "200.0d0" cannot be decoded, so it goes through float)
fn olib_cm v = ( units.decodeValue (((v as float) as string) + "cm") )

-- Real sizes in a StandardUVGen (Bitmap and VRayBitmap).
fn olib_realCoords c wCm hCm =
(
    -- (U_Tiling is left alone: with the real scale on it is the same value as the width)
    try ( c.realWorldScale = true; c.realWorldWidth = olib_cm wCm; c.realWorldHeight = olib_cm hCm ) catch ()
)

-- The maps olib_texNode makes for the material at hand (to link their sizes at the end).
global olib_madeTex = #()

-- Names of the materials in the scene, the compact editor and the Slate views (lower case).
fn olib_usedMatNames =
(
    local out = #()
    try ( for m in sceneMaterials do appendIfUnique out (toLower m.name) ) catch ()
    try ( for m in meditMaterials do appendIfUnique out (toLower m.name) ) catch ()
    try
    (
        for i = 1 to sme.GetNumViews() do
        (
            local v = sme.GetView i
            for j = 1 to v.GetNumNodes() do
                try ( local r = (v.GetNode j).reference; if superClassOf r == material do appendIfUnique out (toLower r.name) ) catch ()
        )
    )
    catch ()
    out
)

-- The material of the scene, the compact editor or Slate called exactly nm (whatever the case).
fn olib_findMaterial nm =
(
    local hit = undefined
    local low = toLower nm
    try ( for m in sceneMaterials while hit == undefined where (toLower m.name) == low do hit = m ) catch ()
    if hit == undefined do
        try ( for m in meditMaterials while hit == undefined where (toLower m.name) == low do hit = m ) catch ()
    if hit == undefined do
    (
        try
        (
            for i = 1 to sme.GetNumViews() while hit == undefined do
            (
                local v = sme.GetView i
                for j = 1 to v.GetNumNodes() while hit == undefined do
                    try ( local r = (v.GetNode j).reference; if superClassOf r == material and (toLower r.name) == low do hit = r ) catch ()
            )
        )
        catch ()
    )
    hit
)

-- Whether that very material is in the scene, the compact editor or Slate already.
fn olib_matKnown m =
(
    local ok = false
    try ( for x in sceneMaterials while not ok where x == m do ok = true ) catch ()
    if not ok do try ( for x in meditMaterials while not ok where x == m do ok = true ) catch ()
    if not ok do
        try
        (
            for i = 1 to sme.GetNumViews() while not ok do
            (
                local v = sme.GetView i
                for j = 1 to v.GetNumNodes() while not ok do try ( ok = (v.GetNode j).reference == m ) catch ()
            )
        )
        catch ()
    ok
)

-- When there is a material of that name, the next free one: Name_01, Name_02...
fn olib_uniqueMatName nm =
(
    local used = olib_usedMatNames()
    if findItem used (toLower nm) == 0 then nm
    else
    (
        local i = 1
        local cand = nm + "_01"
        while findItem used (toLower cand) != 0 do
        (
            i += 1
            cand = nm + "_" + (formattedPrint i format:"02d")
        )
        cand
    )
)

-- Gives the property of every map the same controller: changing the size in one changes it in all
-- (the value the first one had is kept).
fn olib_shareCtrl owners prop ctrlClass =
(
    local c = undefined
    for o in owners do
    (
        try
        (
            local v = getProperty o prop
            if c == undefined then
            (
                c = ctrlClass()
                setPropertyController o prop c
                setProperty o prop v
            )
            else setPropertyController o prop c
        )
        catch ( format "OpenLibrary: % could not be linked: %\n" prop (getCurrentException()) )
    )
)

-- Links the real sizes (or the V-Ray triplanar size) of the maps of a material.
fn olib_linkSizes texs =
(
    local corona = #()
    local coords = #()
    local vtri = #()
    for t in texs where t != undefined do
    (
        if CoronaBitmap != undefined and classOf t == CoronaBitmap then
            ( if t.realWorldScale do append corona t )
        else if VRayTriplanarTex != undefined and classOf t == VRayTriplanarTex then
            append vtri t
        else if hasProperty t #coords do
            try ( if t.coords.realWorldScale do append coords t.coords ) catch ()
    )
    if corona.count > 1 do olib_shareCtrl corona #uvwScale Bezier_Point3
    -- in a StandardUVGen the real width and height are U_Tiling and V_Tiling
    if coords.count > 1 do ( olib_shareCtrl coords #U_Tiling Bezier_Float; olib_shareCtrl coords #V_Tiling Bezier_Float )
    if vtri.count > 1 do olib_shareCtrl vtri #size Bezier_Float
)

-- An image map of the engine. role: "color" or "data" (no gamma: normal, roughness...).
-- mapping "triplanar": projected on the three sides, with a tile of wCm centimetres.
-- mapping "realworld": the texture measures wCm x hCm (for a UVW Map with Real-World Map Size).
fn olib_texNode f engine role mapping wCm hCm =
(
    local t = undefined
    local nm = getFilenameFile f
    if engine == "corona" and CoronaBitmap != undefined then
    (
        t = CoronaBitmap filename:f
        t.name = nm
        if role != "color" do t.gamma = 1.0
        if mapping == "triplanar" do ( t.triplanarMapping = true; t.triplanarScale = olib_cm wCm )
        if mapping == "realworld" do ( t.realWorldScale = true; t.uvwScale = [olib_cm wCm, olib_cm hCm, 1] )
        append olib_madeTex t
    )
    else if engine == "vray" and VRayBitmap != undefined then
    (
        t = VRayBitmap HDRIMapName:f
        t.name = nm
        if role != "color" do t.color_space = 0
        if mapping == "realworld" do olib_realCoords t.coords wCm hCm
        if mapping == "triplanar" do
        (
            local tp = VRayTriplanarTex texture:t size:(olib_cm wCm)
            tp.name = nm + " triplanar"
            t = tp
        )
        append olib_madeTex t
    )
    else
    (
        t = Bitmaptexture fileName:f
        t.name = nm
        if role != "color" do try ( t.bitmap = openBitmap f gamma:1.0 ) catch ()
        if mapping == "realworld" do olib_realCoords t.coords wCm hCm
        append olib_madeTex t
    )
    t
)

-- Puts a map in the shared CoronaTriplanar and returns the output to wire to the material (the main one
-- for the first, Additional Output N for the others). Without a triplanar, the map itself.
fn olib_triWire tri tex =
(
    if tri == undefined or tex == undefined then tex
    else
    (
        local idx = 1
        if tri.texmapX == undefined then tri.texmapX = tex
        else
        (
            tri.additionalOutputs = tri.additionalOutputs + 1
            tri.additionalTexmapX[tri.additionalOutputs] = tex
            idx = tri.additionalOutputs + 1
        )
        local o = MultiOutputChannelTexmapToTexmap sourceMap:tri outputChannelIndex:idx
        o
    )
)

fn olib_specFile keys vals ch =
(
    local i = findItem keys ch
    if i == 0 then undefined else vals[i]
)

-- A PBR material of the engine from its maps.
-- spec: "channel=path|channel=path" with channels color, rough, gloss, refl, metal, normal, normaldx, bump,
-- disp, opacity, emission, transl. Returns the material (assigned to nothing).
fn olib_pbrMaterial matName engine spec mapping wCm hCm useDisp =
(
    local keys = #()
    local vals = #()
    for part in (filterString spec "|") do
    (
        local p = findString part "="
        if p != undefined do ( append keys (substring part 1 (p - 1)); append vals (substring part (p + 1) -1) )
    )
    local fColor = olib_specFile keys vals "color"
    local fRough = olib_specFile keys vals "rough"
    local fGloss = olib_specFile keys vals "gloss"
    local fRefl = olib_specFile keys vals "refl"
    local fMetal = olib_specFile keys vals "metal"
    local fNormal = olib_specFile keys vals "normal"
    local flipG = false
    if fNormal == undefined do ( fNormal = olib_specFile keys vals "normaldx"; flipG = fNormal != undefined )
    local fBump = olib_specFile keys vals "bump"
    local fDisp = olib_specFile keys vals "disp"
    local fOpac = olib_specFile keys vals "opacity"
    local fEmit = olib_specFile keys vals "emission"
    local fTrans = olib_specFile keys vals "transl"
    local m = undefined
    matName = olib_uniqueMatName matName
    olib_madeTex = #()

    if engine == "corona" and CoronaPhysicalMtl != undefined then
    (
        m = CoronaPhysicalMtl name:matName
        -- triplanar: one CoronaTriplanar with every map (the colour in the main input and the rest in the
        -- additional outputs); each channel of the material is wired to its output
        local tri = undefined
        local cm = mapping
        if mapping == "triplanar" and CoronaTriplanar != undefined do
        (
            tri = CoronaTriplanar scale:(olib_cm wCm)
            tri.name = matName + " triplanar"
            cm = "uv"
        )
        if fColor != undefined do m.baseTexmap = olib_triWire tri (olib_texNode fColor engine "color" cm wCm hCm)
        if fRough != undefined then ( m.roughnessMode = 0; m.baseRoughnessTexmap = olib_triWire tri (olib_texNode fRough engine "data" cm wCm hCm) )
        else if fGloss != undefined do ( m.roughnessMode = 1; m.baseRoughnessTexmap = olib_triWire tri (olib_texNode fGloss engine "data" cm wCm hCm) )
        if fMetal != undefined do m.metalnessTexmap = olib_triWire tri (olib_texNode fMetal engine "data" cm wCm hCm)
        if fNormal != undefined then
        (
            local n = CoronaNormal normalMap:(olib_texNode fNormal engine "data" cm wCm hCm)
            n.name = matName + " normal"
            n.flipGreen = flipG
            -- the bump goes in as it is: the normal map goes whole through the triplanar, and wired to it
            -- the bump would close a loop
            if fBump != undefined do ( n.additionalBump = olib_texNode fBump engine "data" cm wCm hCm; n.additionalBumpOn = true )
            m.baseBumpTexmap = olib_triWire tri n
        )
        else if fBump != undefined do m.baseBumpTexmap = olib_triWire tri (olib_texNode fBump engine "data" cm wCm hCm)
        if fDisp != undefined do
        (
            m.displacementTexmap = olib_triWire tri (olib_texNode fDisp engine "data" cm wCm hCm)
            m.displacementTexmapOn = useDisp
            m.displacementMinimum = 0
            m.displacementMaximum = olib_cm 1
        )
        if fOpac != undefined do m.opacityTexmap = olib_triWire tri (olib_texNode fOpac engine "data" cm wCm hCm)
        if fEmit != undefined do ( m.selfIllumTexmap = olib_triWire tri (olib_texNode fEmit engine "color" cm wCm hCm); m.selfIllumLevel = 1 )
        if fTrans != undefined do ( m.translucencyColorTexmap = olib_triWire tri (olib_texNode fTrans engine "color" cm wCm hCm); m.translucencyFraction = 0.5 )
    )
    else if engine == "vray" and VRayMtl != undefined then
    (
        m = VRayMtl name:matName
        if fColor != undefined do m.texmap_diffuse = olib_texNode fColor engine "color" mapping wCm hCm
        if fRough != undefined or fGloss != undefined or fMetal != undefined or fRefl != undefined do m.reflection = white
        if fRough != undefined then ( m.brdf_useRoughness = true; m.texmap_reflectionGlossiness = olib_texNode fRough engine "data" mapping wCm hCm )
        else if fGloss != undefined do ( m.brdf_useRoughness = false; m.texmap_reflectionGlossiness = olib_texNode fGloss engine "data" mapping wCm hCm )
        if fMetal != undefined then m.texmap_metalness = olib_texNode fMetal engine "data" mapping wCm hCm
        else if fRefl != undefined do m.texmap_reflection = olib_texNode fRefl engine "data" mapping wCm hCm
        if fNormal != undefined then
        (
            local n = VRayNormalMap normal_map:(olib_texNode fNormal engine "data" mapping wCm hCm)
            n.name = matName + " normal"
            n.flip_green = flipG
            if fBump != undefined do n.bump_map = olib_texNode fBump engine "data" mapping wCm hCm
            m.texmap_bump = n
            m.texmap_bump_multiplier = 100
        )
        else if fBump != undefined do m.texmap_bump = olib_texNode fBump engine "data" mapping wCm hCm
        if fDisp != undefined do ( m.texmap_displacement = olib_texNode fDisp engine "data" mapping wCm hCm; m.texmap_displacement_on = useDisp )
        if fOpac != undefined do m.texmap_opacity = olib_texNode fOpac engine "data" mapping wCm hCm
        if fEmit != undefined do m.texmap_self_illumination = olib_texNode fEmit engine "color" mapping wCm hCm
        if fTrans != undefined do m.texmap_translucent = olib_texNode fTrans engine "color" mapping wCm hCm
    )
    else
    (
        engine = "max"
        if mapping == "triplanar" do mapping = "uv"
        m = PhysicalMaterial name:matName
        if fColor != undefined do m.base_color_map = olib_texNode fColor engine "color" mapping wCm hCm
        if fRough != undefined then ( m.roughness_inv = false; m.roughness_map = olib_texNode fRough engine "data" mapping wCm hCm )
        else if fGloss != undefined do ( m.roughness_inv = true; m.roughness_map = olib_texNode fGloss engine "data" mapping wCm hCm )
        if fMetal != undefined do m.metalness_map = olib_texNode fMetal engine "data" mapping wCm hCm
        if fNormal != undefined then
        (
            local n = Normal_Bump normal_map:(olib_texNode fNormal engine "data" mapping wCm hCm)
            n.name = matName + " normal"
            n.flipgreen = flipG
            if fBump != undefined do n.bump_map = olib_texNode fBump engine "data" mapping wCm hCm
            m.bump_map = n
            m.bump_map_amt = 1.0
        )
        else if fBump != undefined do m.bump_map = olib_texNode fBump engine "data" mapping wCm hCm
        if fDisp != undefined do ( m.displacement_map = olib_texNode fDisp engine "data" mapping wCm hCm; m.displacement_map_on = useDisp )
        if fOpac != undefined do m.cutout_map = olib_texNode fOpac engine "data" mapping wCm hCm
    )
    olib_linkSizes olib_madeTex
    olib_madeTex = #()
    m
)

-- What the bridge saw under the mouse on the drop (ops.py works it out with the real window):
-- olib_cursorKnown = true and olib_cursorHit = an object or undefined.
global olib_cursorKnown = false
global olib_cursorHit = undefined

-- The visible object in viewport idx, at pixel (x, y) of its window, or undefined. cw x ch is the size of
-- the window in Windows pixels: on a scaled display (125 %, 150 %...) 3ds Max works in logical pixels, so
-- they are brought to its scale with getViewSize.
fn olib_nodeAtView idx x y cw ch =
(
    local best = undefined
    try
    (
        if idx >= 1 and idx <= viewport.numViews and viewport.activeViewport != idx do viewport.activeViewport = idx
        local vs = getViewSize()
        if cw > 0 and ch > 0 do ( x = x * vs.x / cw; y = y * vs.y / ch )
        local r = mapScreenToWorldRay [x, y]
        local bestD = 1e30
        for h in (intersectRayScene r) do
        (
            local nd = h[1]
            if not nd.isHidden and not nd.isFrozen do
            (
                local d = distance r.pos h[2].pos
                if d < bestD do ( bestD = d; best = nd )
            )
        )
    )
    catch ( format "OpenLibrary: viewport % could not be looked at: %\n" idx (getCurrentException()) )
    best
)

-- The visible object under the mouse in the active viewport, or undefined.
fn olib_nodeUnderCursor =
(
    local best = undefined
    try
    (
        local mp = mouse.pos
        local vs = getViewSize()
        if mp.x >= 0 and mp.y >= 0 and mp.x <= vs.x and mp.y <= vs.y do
        (
            local r = mapScreenToWorldRay mp
            local bestD = 1e30
            for h in (intersectRayScene r) do
            (
                local nd = h[1]
                if not nd.isHidden and not nd.isFrozen do
                (
                    local d = distance r.pos h[2].pos
                    if d < bestD do ( bestD = d; best = nd )
                )
            )
        )
    )
    catch ()
    best
)

-- Puts materials or maps in the active view of the Slate editor. Returns how many.
fn olib_toSlate items =
(
    local n = 0
    try
    (
        if not (sme.IsOpen()) do sme.Open()
        local v = undefined
        try ( v = sme.GetView sme.activeView ) catch ()
        if v == undefined do ( sme.CreateView "OpenLibrary"; v = sme.GetView (sme.GetNumViews()) )
        -- to the right of what the view has (maps are placed to the left of their material)
        local x0 = 0
        local y0 = 0
        local first = true
        for i = 1 to v.GetNumNodes() do
        (
            local p = undefined
            try ( p = (v.GetNode i).position ) catch ()
            if p != undefined do
            (
                if first or p.x > x0 do x0 = p.x
                if first or p.y < y0 do y0 = p.y
                first = false
            )
        )
        if not first do x0 += 700
        local k = 0
        for it in items do
        (
            v.CreateNode it [x0 + (mod k 4) * 700, y0 + ((k / 4) as integer) * 500]
            k += 1
            n += 1
        )
        try ( v.SetSelectedNodes items ) catch ()
        try ( v.ZoomExtents type:#selected ) catch ()
        try ( sme.SetMtlInParamEditor items[items.count] ) catch ()
    )
    catch ( format "OpenLibrary: the Slate editor could not be used: %\n" (getCurrentException()) )
    n
)

-- Puts materials in Slate using the nodes there are: when all of them are in one view, that view is shown
-- and they are selected; otherwise those missing from the active view are added to it.
-- Returns how many are new.
fn olib_slateShow mats =
(
    local added = 0
    try
    (
        if not (sme.IsOpen()) do sme.Open()
        local inView = #()          -- the view of each material (0 = none)
        for m in mats do
        (
            local w = 0
            for i = 1 to sme.GetNumViews() while w == 0 do
            (
                local v = sme.GetView i
                for j = 1 to v.GetNumNodes() while w == 0 do try ( if (v.GetNode j).reference == m do w = i ) catch ()
            )
            append inView w
        )
        local same = inView.count > 0 and inView[1] > 0
        for w in inView where w != inView[1] do same = false
        if same then
            try ( sme.activeView = inView[1] ) catch ()
        else
        (
            local v = sme.GetView sme.activeView
            local missing = #()
            for k = 1 to mats.count where inView[k] != sme.activeView do append missing mats[k]
            if missing.count > 0 do added = olib_toSlate missing
        )
        local v = sme.GetView sme.activeView
        try ( v.SetSelectedNodes mats ) catch ()
        try ( v.ZoomExtents type:#selected ) catch ()
        try ( sme.SetMtlInParamEditor mats[mats.count] ) catch ()
    )
    catch ( format "OpenLibrary: the Slate editor could not be used: %\n" (getCurrentException()) )
    added
)

-- A mapping box on each object. mode "box": a box of X x Y x Z cm (width, length, height).
-- mode "realworld": a box with Real-World Map Size (the texture gives the size).
fn olib_realUvw nodes mode xCm yCm zCm =
(
    for n in nodes do
    (
        try
        (
            local u = undefined
            for md in n.modifiers where md.name == "OpenLibrary UVW" do u = md
            if u == undefined do ( u = Uvwmap name:"OpenLibrary UVW"; addModifier n u )
            u.maptype = 4
            if mode == "realworld" then
                u.realWorldMapSize = true
            else
            (
                u.realWorldMapSize = false
                u.width = olib_cm xCm
                u.length = olib_cm yCm
                u.height = olib_cm zCm
            )
        )
        catch ()
    )
)

-- Whether the mouse is over the open Slate editor (3ds Max decides: its window may be docked).
fn olib_mouseOverSlate =
(
    local r = false
    try
    (
        if sme.IsOpen() do
        (
            local mf = sme.GetMainframe()
            if mf.visible do
            (
                local p = mouse.screenpos
                local a = mf.position
                local s = mf.size
                r = p.x >= a.x and p.y >= a.y and p.x <= a.x + s.x and p.y <= a.y + s.y
            )
        )
    )
    catch ()
    r
)

-- When the Slate editor is open, what was put on the objects is left there too.
fn olib_alsoSlate mats =
(
    local open = false
    try ( open = sme.IsOpen() ) catch ()
    if open then olib_toSlate mats else 0
)

-- The target objects: the one under the mouse (with the whole selection when it is part of it) or the selection.
fn olib_targetNodes target =
(
    local nodes = #()
    if target == "cursor" do
    (
        local hit = if olib_cursorKnown then olib_cursorHit else olib_nodeUnderCursor()
        olib_cursorKnown = false
        olib_cursorHit = undefined
        if hit != undefined do
            nodes = if hit.isSelected then (selection as array) else #(hit)
    )
    if target != "slate" and target != "plane" and nodes.count == 0 do nodes = selection as array
    nodes
)

-- Deals materials out. target: "selection", "cursor" (the object under the mouse, or the selection) or
-- "slate". uvwMode "box" / "realworld" adds the mapping box (X, Y, Z in cm).
-- Returns "objects|N|name", "slate|N|name" or "none|0|name".
fn olib_putMaterials mats target uvwMode xCm yCm zCm =
(
    local out = "none|0|"
    if mats.count > 0 do
    (
        -- (new ones, from a .mat library say, get a name of their own when the scene has one like it;
        -- one that was in the scene stays as it is)
        for m in mats where not (olib_matKnown m) do try ( m.name = olib_uniqueMatName m.name ) catch ()
        local nodes = olib_targetNodes target
        if target == "slate" then
            out = "slate|" + ((olib_toSlate mats) as string) + "|" + mats[1].name
        else if nodes.count == 0 then
            out = "none|0|" + mats[1].name
        else
        (
            for n in nodes do try ( n.material = mats[1] ) catch ()
            if uvwMode == "box" or uvwMode == "realworld" do olib_realUvw nodes uvwMode xCm yCm zCm
            -- with the editor open, the material shows in its active view too
            local w = if (olib_alsoSlate #(mats[1])) > 0 then "objects+slate" else "objects"
            out = w + "|" + (nodes.count as string) + "|" + mats[1].name
        )
    )
    out
)

-- ---------------------------------------------------------------------------
-- Fake interiors (wParallax / ShadersBOX): an OSL map reads the room's EXR and goes to the self-illumination
-- of a black material; the preview (jpg) is only seen in the viewport.
-- sizeCode "S" / "M" / "L" (the window's width); depthIdx -1 = the depth of the sample scenes.
-- ---------------------------------------------------------------------------
fn olib_interiorMaterial matName engine osl exr preview sizeCode depthIdx =
(
    matName = olib_uniqueMatName matName
    local o = OSLMap()
    o.OSLPath = osl
    o.OSLAutoUpdate = true
    o.name = matName + " OSL"
    try ( o.filename = exr ) catch ()
    local vals = #(0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0)
    local aIdx = case sizeCode of ( "M": 4; "L": 5; default: 1 )
    local dIdx = if depthIdx >= 0 then depthIdx else (if sizeCode == "S" then 3 else 5)
    -- wParallax_SMP / ADV use drop-downs (indices); jiWindowBox, values
    if matchPattern (filenameFromPath osl) pattern:"wParallax*" then
        ( try ( o.roomDepth = dIdx; o.windowAspect = aIdx ) catch () )
    else
        ( try ( o.roomDepth = vals[dIdx + 1]; o.windowAspect = vals[aIdx + 1] ) catch () )
    local pv = undefined
    if preview != "" do ( pv = Bitmaptexture fileName:preview; pv.name = matName + " preview" )
    local m = undefined
    if engine == "corona" and CoronaPhysicalMtl != undefined then
    (
        m = CoronaPhysicalMtl name:matName
        m.baseColor = black
        m.selfIllumTexmap = o
        m.selfIllumLevel = 1.5
        if pv != undefined do ( m.baseTexmap = pv; m.baseTexmapOn = false )
    )
    else if engine == "vray" and VRayMtl != undefined then
    (
        m = VRayMtl name:matName
        m.diffuse = black
        m.texmap_self_illumination = o
        m.selfIllumination_multiplier = 1.5
        if pv != undefined do ( m.texmap_diffuse = pv; m.texmap_diffuse_on = false )
    )
    else
    (
        m = PhysicalMaterial name:matName
        m.base_color = black
        m.emit_color_map = o
        m.emission = 1.0
    )
    if pv != undefined do try ( showTextureMap m pv true ) catch ()
    m
)

-- The window's upright plane (like those of the sample scenes), where the mouse is on the construction
-- plane or, when the mouse is on no viewport, at the origin.
fn olib_interiorPlane mat wCm hCm =
(
    local pt = [0, 0, 0]
    try
    (
        local mp = mouse.pos
        local vs = getViewSize()
        if mp.x >= 0 and mp.y >= 0 and mp.x <= vs.x and mp.y <= vs.y do pt = gw.getPointOnCP mp
    )
    catch ()
    local p = Plane length:(olib_cm hCm) width:(olib_cm wCm) lengthsegs:1 widthsegs:1
    p.rotation = quat 0.707107 0 0 0.707107
    p.pos = pt + [0, 0, (olib_cm hCm) / 2.0]
    p.name = uniqueName ("OL_" + mat.name + "_")
    p.material = mat
    select p
    p
)

-- target: "selection", "cursor", "slate" or "plane" (makes the window). Without target objects the plane is
-- made, but for "selection". Returns "objects|N|name", "plane|1|name", "slate|N|name" or "none|0|name".
fn olib_putInterior mat target wCm hCm =
(
    local nodes = olib_targetNodes target
    if target == "slate" then
        "slate|" + ((olib_toSlate #(mat)) as string) + "|" + mat.name
    else if nodes.count > 0 then
    (
        for n in nodes do try ( n.material = mat ) catch ()
        local w = if (olib_alsoSlate #(mat)) > 0 then "objects+slate" else "objects"
        w + "|" + (nodes.count as string) + "|" + mat.name
    )
    else if target == "selection" then
        "none|0|" + mat.name
    else
    (
        olib_interiorPlane mat wCm hCm
        "plane|1|" + mat.name
    )
)

-- The materials of a .mat library (without opening it in the editor).
fn olib_matLibMaterials f =
(
    local out = #()
    try ( local lib = loadTempMaterialLibrary f; if lib != undefined do for m in lib do append out m ) catch ()
    out
)

format "OpenLibrary: materials loaded\n"
