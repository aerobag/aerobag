package net.jonh.aerobag.prototype.domain

interface NativeBridge {
    fun replaceFlightPlanStateJson(
        stateJson: String,
        catalogJson: String,
        planJson: String,
    ): String

    fun setContentPolicyStateJson(
        stateJson: String,
        catalogJson: String,
        policyJson: String,
    ): String

    fun refreshContentStateJson(
        stateJson: String,
        catalogJson: String,
        inventoryJson: String,
    ): String

    fun chartForPositionJson(
        catalogJson: String,
        geometryJson: String,
        familyJson: String,
        lat: Double,
        lon: Double,
    ): String
}

object NativeBindings : NativeBridge {
    init {
        System.loadLibrary("app_ffi")
    }

    external override fun replaceFlightPlanStateJson(
        stateJson: String,
        catalogJson: String,
        planJson: String,
    ): String

    external override fun setContentPolicyStateJson(
        stateJson: String,
        catalogJson: String,
        policyJson: String,
    ): String

    external override fun refreshContentStateJson(
        stateJson: String,
        catalogJson: String,
        inventoryJson: String,
    ): String

    external override fun chartForPositionJson(
        catalogJson: String,
        geometryJson: String,
        familyJson: String,
        lat: Double,
        lon: Double,
    ): String
}
