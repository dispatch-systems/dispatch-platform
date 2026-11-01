package com.dspdispatch.relay.setup

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MakerScreensTest {
    private val makers = listOf(
        "samsung", "xiaomi", "redmi", "poco", "huawei", "honor", "oppo", "realme", "oneplus", "vivo", "iqoo",
        "asus", "lenovo", "motorola", "tecno", "infinix", "itel", "meizu", "letv", "htc", "hmd global",
    )
    private val controls = makers.flatMap(MakerScreens::controls)

    @Test
    fun `finds a maker's controls whatever the case Android gives its name in`() {
        assertEquals(MakerScreens.controls("samsung"), MakerScreens.controls(" Samsung "))
        assertTrue(MakerScreens.controls("Xiaomi").first().screens.first().startsWith("com.miui.securitycenter/"))
    }

    @Test
    fun `has none for a maker that adds no control of its own`() {
        assertEquals(emptyList<Control>(), MakerScreens.controls("Google"))
    }

    @Test
    fun `offers Xiaomi's autostart and its battery saver each on its own`() {
        val xiaomi = MakerScreens.controls("xiaomi")
        assertEquals(listOf("autostart", "battery_saver"), xiaomi.map(Control::name))
        assertEquals("com.miui.powerkeeper", xiaomi.last().screens.single().substringBefore('/'))
    }

    @Test
    fun `opens Samsung's Never sleeping apps by its documented action, its screens after`() {
        val samsung = MakerScreens.controls("samsung").single()
        assertEquals(
            Action("com.samsung.android.lool", "com.samsung.android.sm.ACTION_OPEN_CHECKABLE_LISTACTIVITY", mapOf("activity_type" to 2)),
            samsung.action,
        )
        assertTrue(samsung.screens.isNotEmpty())
    }

    @Test
    fun `names each control once per maker`() {
        makers.forEach { maker -> MakerScreens.controls(maker).map(Control::name).let { assertEquals(maker, it.distinct(), it) } }
    }

    @Test
    fun `names each screen as package and class`() {
        controls.flatMap(Control::screens).forEach { assertTrue(it, Regex("[\\w.]+/[\\w.]+").matches(it)) }
    }

    @Test
    fun `can see each maker's package, as the manifest's queries let it`() {
        val manifest = File("src/main/AndroidManifest.xml").readText()
        val queried = Regex("<package android:name=\"([^\"]+)\"").findAll(manifest).map { it.groupValues[1] }.toSet()
        val needed = (controls.flatMap(Control::screens).map { it.substringBefore('/') } + controls.mapNotNull { it.action?.pkg }).toSet()
        assertEquals(needed, queried)
    }
}
