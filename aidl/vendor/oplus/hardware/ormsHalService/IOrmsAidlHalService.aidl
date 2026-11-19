// AIDL for the OPlus ORMS (Oplus Resource Manager) HAL (V1).
//
// NOTE: the interface IS VINTF stable. The backend exports
// BnOrmsAidlHalService::getInterfaceVersion/getInterfaceHash and references
// AIBinder_markVintfStability@LIBBINDER_NDK30, which is what the
// @VintfStability annotation generates.
//
// Method order is load-bearing: AIDL assigns transaction codes by declaration
// order (FIRST_CALL_TRANSACTION == 1), and dispatch follows exactly this order.
//
// Argument names are reproduced verbatim from the API definition, including the
// upstream "stingValue" spelling, so the generated API signature matches.
package vendor.oplus.hardware.ormsHalService;

@VintfStability
interface IOrmsAidlHalService {
    void ormsBoostAcquire(int pl_handle, in int[] boostsList);
    void ormsBoostRelease(int pl_handle);
    void ormsEnableCpuBouncing(String stingValue);
    String ormsReadDdrAvailFreq(int cluster);
    String ormsReadFile(String path);
    String ormsReadGpuFreq(int pos);
    void ormsWriteAboveHispeedDelay(int setOrRelease, int cluster, String stingValue);
    void ormsWriteBgCpuUclampMin(String stingValue);
    void ormsWriteBusyDownThres(String stingValue);
    void ormsWriteBusyUpThres(String stingValue);
    void ormsWriteCameraTracingEvents(String stingValue);
    void ormsWriteCoreCtlEnable(String stingValue);
    void ormsWriteCpuBouncing(String stingValue);
    void ormsWriteCpuCoreNum(int cluster, int min, int max);
    void ormsWriteCpuDdrLatfloorMax(int cluster, String stingValue);
    void ormsWriteCpuDdrLatfloorMax2(int clusterOffset, String stingValue);
    void ormsWriteCpuDdrLatfloorMin(int cluster, String stingValue);
    void ormsWriteCpuDdrLatfloorMin2(int clusterOffset, String stingValue);
    void ormsWriteCpuL3LatMax(int cluster, String stingValue);
    void ormsWriteCpuLlccLatMax(int cluster, String stingValue);
    void ormsWriteCpuOnline(int checkMask, int onlineMask);
    void ormsWriteFgCpuUclampMin(String stingValue);
    void ormsWriteFpsgo(String stingValue);
    void ormsWriteHwmonHystOpt(String stingValue);
    void ormsWriteInputBoostEnabled(String stingValue);
    void ormsWriteInputBoostFreq(String stingValue);
    void ormsWriteLlccDdrLatMax(int cluster, String stingValue);
    void ormsWriteLowPowerMode(String stingValue);
    void ormsWriteMemlatL3Opt(String stingValue);
    void ormsWritePreferSilverEnabled(String stingValue);
    void ormsWriteRulerEnable(String stingValue);
    void ormsWriteSchedtuneColocate(String stingValue);
    void ormsWriteSchedtunePreferIdle(String stingValue);
    void ormsWriteSleepDisabled(String stingValue);
    void ormsWriteSlideBoost(String stingValue);
    void ormsWriteTargetLoads(int setOrRelease, int cluster, String stingValue);
    void ormsWriteTopCpuUclampMin(String stingValue);
    void ormsWriteTouchBoost(String stingValue);
    void ormsWriteTracingSetEvent(String stingValue);
    void ormsWriteUclampLatencySensitive(String stingValue);
    void ormsWritehalUfsPlusCtrl(String stingValue1, String stingValue2);
    void ormsWriteSchedAsymcapBoost(String stingValue);
    void ormsWriteCpuCpuDdrLatMin(int cluster, String stingValue2);
    void ormsWriteCpuCpuDdrBwMin(String stingValue);
    void ormsWriteNodeCommon(int id, int cluster, String stingValue);
    void ormsWriteForceStep(String stingValue);
}
