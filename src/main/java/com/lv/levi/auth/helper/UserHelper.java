package com.lv.levi.auth.helper;

import java.util.UUID;

public class UserHelper {
    public static UUID parseUUID(String uuid) {
        return UUID.fromString(uuid);
    }
}
