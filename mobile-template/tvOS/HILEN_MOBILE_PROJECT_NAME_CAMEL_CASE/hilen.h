#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>

int hilen_start_app(void);

void hilen_ios_show_alert(const char* message) {

    NSString* ns_message = [NSString stringWithUTF8String:message];

    UIAlertController *alertController = [UIAlertController alertControllerWithTitle:nil
                                                                             message:ns_message
                                                                      preferredStyle:UIAlertControllerStyleAlert];

    UIAlertAction *okAction = [UIAlertAction actionWithTitle:@"OK"
                                                       style:UIAlertActionStyleDefault
                                                     handler:nil];

    [alertController addAction:okAction];

    UIViewController* controller = [UIApplication sharedApplication].keyWindow.rootViewController;
    [controller presentViewController:alertController animated:YES completion:nil];
}

// tvOS has no iCloud document storage.
const char* hilen_ios_get_icloud_storage_path(void) {
    return NULL;
}
