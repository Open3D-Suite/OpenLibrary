// SPDX-License-Identifier: GPL-3.0-or-later
//
// OpenExample's plug-in for 3ds Max: the entry points 3ds Max looks for in a plug-in, a global utility
// plug-in that 3ds Max starts with itself, and the MAXScript interface it owns:
//
//     OpenExample.describe()   -- the folders where OpenExample keeps its files
//
// What can be tested without 3ds Max is in src/, in the openexample_core library.

#include "folders.h"

#include <max.h>
#include <iparamb2.h>
#include <ifnpub.h>
#include <guplib.h>

// The ids 3ds Max knows OpenExample by. Each is two random 32-bit numbers made once for this plug-in and
// never changed, such as Class_ID(0x1a2b3c4d, 0x5e6f7a8b); the 3ds Max SDK's gencid.exe makes them. Until
// they are written here the plug-in does not build.
#define OPENEXAMPLE_CLASS_ID Class_ID(REPLACE_WITH_TWO_NEW_NUMBERS)
#define OPENEXAMPLE_INTERFACE_ID Interface_ID(REPLACE_WITH_TWO_NEW_NUMBERS)

HINSTANCE hInstance = nullptr;

namespace
{

// The global utility plug-in. 3ds Max starts it with itself and keeps it loaded, so the interface below
// is there from the start.
class OpenExampleGup : public GUP
{
  public:
    DWORD Start() override
    {
        return GUPRESULT_KEEP;
    }

    void Stop() override
    {
    }

    void DeleteThis() override
    {
        delete this;
    }
};

class OpenExampleClassDesc : public ClassDesc2
{
  public:
    int IsPublic() override
    {
        return TRUE;
    }
    void* Create(BOOL) override
    {
        return new OpenExampleGup();
    }
    const MCHAR* ClassName() override
    {
        return _T("OpenExample plug-in");
    }
    const MCHAR* NonLocalizedClassName() override
    {
        return _T("OpenExample plug-in");
    }
    SClass_ID SuperClassID() override
    {
        return GUP_CLASS_ID;
    }
    Class_ID ClassID() override
    {
        return OPENEXAMPLE_CLASS_ID;
    }
    const MCHAR* Category() override
    {
        return _T("Open3D");
    }
    const MCHAR* InternalName() override
    {
        return _T("OpenExamplePlugin");
    }
    HINSTANCE HInstance() override
    {
        return hInstance;
    }
};

ClassDesc2* classDesc()
{
    static OpenExampleClassDesc desc;
    return &desc;
}

// The MAXScript interface, OpenExample, owned by the plug-in above.
class OpenExampleInterface : public FPStaticInterface
{
  public:
    enum FunctionId
    {
        kDescribe,
    };

    // The folders where OpenExample keeps its files.
    MSTR describe()
    {
        return MSTR(openexample::describe_here().c_str());
    }

    DECLARE_DESCRIPTOR(OpenExampleInterface)
    BEGIN_FUNCTION_MAP
    FN_0(kDescribe, TYPE_TSTR_BV, describe)
    END_FUNCTION_MAP
};

OpenExampleInterface theInterface(OPENEXAMPLE_INTERFACE_ID, _T("OpenExample"), 0, classDesc(), FP_CORE,
                                  OpenExampleInterface::kDescribe, _T("describe"), 0, TYPE_TSTR_BV, 0, 0, p_end);

} // namespace

BOOL WINAPI DllMain(HINSTANCE module, ULONG reason, LPVOID)
{
    if (reason == DLL_PROCESS_ATTACH)
    {
        hInstance = module;
        DisableThreadLibraryCalls(module);
    }
    return TRUE;
}

__declspec(dllexport) const MCHAR* LibDescription()
{
    return _T("OpenExample ") _T(OPENEXAMPLE_VERSION) _T(" - a plug-in for 3ds Max (Open3D)");
}

__declspec(dllexport) int LibNumberClasses()
{
    return 1;
}

__declspec(dllexport) ClassDesc* LibClassDesc(int i)
{
    return i == 0 ? classDesc() : nullptr;
}

__declspec(dllexport) ULONG LibVersion()
{
    return VERSION_3DSMAX;
}

// The interface must be there when 3ds Max starts, so the plug-in is never loaded on demand.
__declspec(dllexport) ULONG CanAutoDefer()
{
    return 0;
}
