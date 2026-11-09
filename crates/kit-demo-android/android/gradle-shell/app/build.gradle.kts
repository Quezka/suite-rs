// The Android shell around a Rust app: GameActivity (for proper keyboard input) and the app's native library,
// built by ../build.sh into src/main/jniLibs. No Java or Kotlin of our own.
plugins {
    id("com.android.application")
}

android {
    namespace = "io.github.quezka.kitdemo"
    compileSdk = 35
    ndkVersion = "27.2.12479018"

    defaultConfig {
        applicationId = "io.github.quezka.kitdemo"
        minSdk = 26
        targetSdk = 34
        versionCode = 2
        versionName = "0.2.0"
    }

    signingConfigs {
        create("demo") {
            storeFile = file(System.getProperty("user.home") + "/.local/share/quezka-kitdemo/debug.jks")
            storePassword = "kitdemo"
            keyAlias = "kitdemo"
            keyPassword = "kitdemo"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("demo")
        }
    }

    packaging {
        jniLibs.useLegacyPackaging = false
    }
}

dependencies {
    // One Kotlin standard library: the old jdk7/jdk8 parts are folded into it.
    implementation(platform("org.jetbrains.kotlin:kotlin-bom:1.8.22"))
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.core:core:1.13.1")
    implementation("androidx.games:games-activity:4.4.0")
}
