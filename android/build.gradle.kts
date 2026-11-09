allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

val newBuildDir: Directory =
    rootProject.layout.buildDirectory
        .dir("../../build")
        .get()
rootProject.layout.buildDirectory.value(newBuildDir)

subprojects {
    val newSubprojectBuildDir: Directory = newBuildDir.dir(project.name)
    project.layout.buildDirectory.value(newSubprojectBuildDir)
}
subprojects {
    if (name != "app") {
        // desktop_drop still pins compileSdk 33; AndroidX metadata requires 34+.
        afterEvaluate {
            val android = extensions.findByName("android") ?: return@afterEvaluate
            val setter = android.javaClass.methods.firstOrNull { method ->
                method.name == "setCompileSdk" && method.parameterCount == 1 &&
                    (method.parameterTypes[0] == Int::class.javaPrimitiveType ||
                        method.parameterTypes[0].name == "java.lang.Integer")
            } ?: return@afterEvaluate
            setter.invoke(android, 36)
        }
    }
    project.evaluationDependsOn(":app")
}

tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
