plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.mozilla.rust-android-gradle.rust-android")
}

android {
    namespace = "com.example.HILEN_MOBILE_PROJECT_NAME_SNAKE_CASE"
    compileSdk = 34
    ndkVersion = "27.0.11718014"

    defaultConfig {
        applicationId = "com.example.HILEN_MOBILE_PROJECT_NAME_SNAKE_CASE"
        // AAudio, which cpal links against, exists since API 26. Linking a
        // lower minSdk against it only moves the failure to .so load time.
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        viewBinding = true
    }
    sourceSets {
        getByName("androidTest") {
            jniLibs.srcDir("$buildDir/rustJniLibs/android")
        }
        getByName("debug") {
            jniLibs.srcDir("$buildDir/rustJniLibs/android")
        }
    }
}

cargo {

    profile = "release"
    pythonCommand = "python3"

    targetDirectory = "../../../target"
    module  = "../../../HILEN_MOBILE_PROJECT_NAME_KEBAB_CASE-android"

    libname = "HILEN_MOBILE_PROJECT_NAME_SNAKE_CASE"
    targets = listOf("x86_64", "x86", "arm", "arm64")
}

tasks.whenTaskAdded {
    if (name == "javaPreCompileDebug" || name == "javaPreCompileRelease") {
        dependsOn("cargoBuild")
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.games:games-activity:2.0.2")
    implementation("androidx.appcompat:appcompat:1.6.1")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.5")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.1")
}