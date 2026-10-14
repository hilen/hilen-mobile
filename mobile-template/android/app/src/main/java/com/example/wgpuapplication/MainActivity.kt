package com.example.HILEN_MOBILE_PROJECT_NAME_SNAKE_CASE

import android.content.Intent
import android.content.res.AssetManager
import android.os.Bundle
import android.view.View
import com.google.androidgamesdk.GameActivity

class MainActivity : GameActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        supportActionBar?.hide();
        window.decorView.setSystemUiVisibility(View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
        or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
        or View.SYSTEM_UI_FLAG_FULLSCREEN
        or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY);

        setFilesDir(filesDir.toString())
        setAssetManager(getResources().assets);
    }

    // A link while the app runs comes here, the activity is singleTask
    // when hilen.toml has a [links] table. The engine reads the intent of
    // the activity when the app comes to the front.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
    }

    private external fun setAssetManager(assetManager: AssetManager);
    private external fun setFilesDir(message: String);


    companion object {
        init {
            System.loadLibrary("HILEN_MOBILE_PROJECT_NAME_SNAKE_CASE")
        }
    }
}