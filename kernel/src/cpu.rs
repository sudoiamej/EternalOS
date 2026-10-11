use core::arch::x86_64::__cpuid;

#[derive(Debug, Clone, Copy)]
pub struct CpuFeatures {
    pub fpu: bool,
    pub tsc: bool,
    pub msr: bool,
    pub pae: bool,
    pub apic: bool,
    pub sse: bool,
    pub sse2: bool,
    pub sse3: bool,
    pub ssse3: bool,
    pub sse4_1: bool,
    pub sse4_2: bool,
    pub xsave: bool,
    pub avx: bool,
    pub rdrand: bool,
    // Extended features (leaf 0x80000001)
    pub long_mode: bool,
    pub nx: bool,
    pub gigabyte_pages: bool,
    pub rdtscp: bool,
}

impl CpuFeatures {
    pub fn detect() -> Self {
        // Leaf 1: Standard Features
        let leaf1 = __cpuid(1);
        let edx = leaf1.edx;
        let ecx = leaf1.ecx;

        let fpu = (edx & (1 << 0)) != 0;
        let tsc = (edx & (1 << 4)) != 0;
        let msr = (edx & (1 << 5)) != 0;
        let pae = (edx & (1 << 6)) != 0;
        let apic = (edx & (1 << 9)) != 0;
        let sse = (edx & (1 << 25)) != 0;
        let sse2 = (edx & (1 << 26)) != 0;

        let sse3 = (ecx & (1 << 0)) != 0;
        let ssse3 = (ecx & (1 << 9)) != 0;
        let sse4_1 = (ecx & (1 << 19)) != 0;
        let sse4_2 = (ecx & (1 << 20)) != 0;
        let xsave = (ecx & (1 << 26)) != 0;
        let avx = (ecx & (1 << 28)) != 0;
        let rdrand = (ecx & (1 << 30)) != 0;

        // Leaf 0x80000001: Extended Features
        let ext_leaf = __cpuid(0x8000_0001);
        let ext_edx = ext_leaf.edx;

        let nx = (ext_edx & (1 << 20)) != 0;
        let gigabyte_pages = (ext_edx & (1 << 26)) != 0;
        let rdtscp = (ext_edx & (1 << 27)) != 0;
        let long_mode = (ext_edx & (1 << 29)) != 0;

        Self {
            fpu,
            tsc,
            msr,
            pae,
            apic,
            sse,
            sse2,
            sse3,
            ssse3,
            sse4_1,
            sse4_2,
            xsave,
            avx,
            rdrand,
            long_mode,
            nx,
            gigabyte_pages,
            rdtscp,
        }
    }
}

pub fn log_cpu_capabilities() {
    let feat = CpuFeatures::detect();
    crate::log_info!("x86-64 Hardware Features (CPUID):");
    crate::log_info!("  Base Features : FPU={}, TSC={}, MSR={}, PAE={}, APIC={}", feat.fpu, feat.tsc, feat.msr, feat.pae, feat.apic);
    crate::log_info!("  SIMD/Vector   : SSE={}, SSE2={}, SSE3={}, SSSE3={}, SSE4.1={}, SSE4.2={}, AVX={}", feat.sse, feat.sse2, feat.sse3, feat.ssse3, feat.sse4_1, feat.sse4_2, feat.avx);
    crate::log_info!("  Extended Arch : LongMode={}, NX={}, 1GB_Pages={}, RDTSCP={}", feat.long_mode, feat.nx, feat.gigabyte_pages, feat.rdtscp);
    crate::log_info!("  SIMD Policy   : Soft-float only (kernel preemptions do not clobber SSE/AVX)");
}
