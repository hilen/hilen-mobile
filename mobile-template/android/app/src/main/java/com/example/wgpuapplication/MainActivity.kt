package com.example.HILEN_MOBILE_PROJECT_NAME_SNAKE_CASE

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

    private external fun setAssetManager(assetManager: AssetManager);
    private external fun setFilesDir(message: String);


    companion object {
        init {
            System.loadLibrary("HILEN_MOBILE_PROJECT_NAME_SNAKE_CASE")
        }
    }
}