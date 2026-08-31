#include <CoreFoundation/CoreFoundation.h>
#include <Security/Security.h>
#include <stdio.h>
#include <string.h>

static const char *service = "lam.synthetic-probe";
static const char *account = "credential/synthetic-probe/v1";
static const char *value = "synthetic-value";
static const char *group = "dev.localagentmanager.desktop.shared";

static CFMutableDictionaryRef query(int shared) {
    CFMutableDictionaryRef item = CFDictionaryCreateMutable(
        kCFAllocatorDefault, 0, &kCFTypeDictionaryKeyCallBacks,
        &kCFTypeDictionaryValueCallBacks);
    CFDictionarySetValue(item, kSecClass, kSecClassGenericPassword);
    CFStringRef service_value = CFStringCreateWithCString(kCFAllocatorDefault, service, kCFStringEncodingUTF8);
    CFStringRef account_value = CFStringCreateWithCString(kCFAllocatorDefault, account, kCFStringEncodingUTF8);
    CFDictionarySetValue(item, kSecAttrService, service_value);
    CFDictionarySetValue(item, kSecAttrAccount, account_value);
    if (shared) {
        CFStringRef group_value = CFStringCreateWithCString(kCFAllocatorDefault, group, kCFStringEncodingUTF8);
        CFDictionarySetValue(item, kSecAttrAccessGroup, group_value);
        CFDictionarySetValue(item, kSecUseDataProtectionKeychain, kCFBooleanTrue);
        CFRelease(group_value);
    }
    CFRelease(service_value);
    CFRelease(account_value);
    return item;
}

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    int shared = strcmp(argv[2], "shared") == 0;
    CFMutableDictionaryRef item = query(shared);
    OSStatus status = errSecSuccess;
    if (strcmp(argv[1], "write") == 0) {
        CFDataRef data = CFDataCreate(kCFAllocatorDefault, (const UInt8 *)value, strlen(value));
        CFDictionarySetValue(item, kSecValueData, data);
        status = SecItemAdd(item, NULL);
        CFRelease(data);
    } else if (strcmp(argv[1], "read") == 0) {
        CFDictionarySetValue(item, kSecReturnData, kCFBooleanTrue);
        CFDictionarySetValue(item, kSecMatchLimit, kSecMatchLimitOne);
        CFTypeRef result = NULL;
        status = SecItemCopyMatching(item, &result);
        if (result) CFRelease(result);
    } else if (strcmp(argv[1], "delete") == 0) {
        status = SecItemDelete(item);
    } else {
        status = errSecParam;
    }
    CFRelease(item);
    printf("%d\n", (int)status);
    return status == errSecSuccess ? 0 : 1;
}
