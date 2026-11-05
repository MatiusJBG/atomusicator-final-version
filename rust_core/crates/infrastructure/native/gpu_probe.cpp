#include <dxgi1_4.h>
#include <wrl/client.h>
#include <cstdint>
#include <cstring>

struct AtomusicatorGpuMemory {
    uint32_t index, vendor, device, software;
    uint64_t dedicated, shared, budget, usage;
    uint32_t budget_supported;
    uint16_t name[128];
};

extern "C" int32_t atomusicator_gpu_memory(uint32_t index, AtomusicatorGpuMemory *output) {
    if (!output) return E_POINTER;
    *output = {};
    Microsoft::WRL::ComPtr<IDXGIFactory1> factory;
    HRESULT hr = CreateDXGIFactory1(IID_PPV_ARGS(&factory));
    if (FAILED(hr)) return hr;
    Microsoft::WRL::ComPtr<IDXGIAdapter1> adapter;
    hr = factory->EnumAdapters1(index, &adapter);
    if (hr == DXGI_ERROR_NOT_FOUND) return 1;
    if (FAILED(hr)) return hr;
    DXGI_ADAPTER_DESC1 desc = {};
    hr = adapter->GetDesc1(&desc);
    if (FAILED(hr)) return hr;
    output->index = index;
    output->vendor = desc.VendorId;
    output->device = desc.DeviceId;
    output->software = !!(desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE);
    output->dedicated = desc.DedicatedVideoMemory;
    output->shared = desc.SharedSystemMemory;
    static_assert(sizeof(desc.Description) == sizeof(output->name));
    std::memcpy(output->name, desc.Description, sizeof(output->name));
    Microsoft::WRL::ComPtr<IDXGIAdapter3> memory_adapter;
    if (SUCCEEDED(adapter.As(&memory_adapter))) {
        DXGI_QUERY_VIDEO_MEMORY_INFO info = {};
        if (SUCCEEDED(memory_adapter->QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &info))) {
            output->budget = info.Budget;
            output->usage = info.CurrentUsage;
            output->budget_supported = 1;
        }
    }
    return 0;
}
