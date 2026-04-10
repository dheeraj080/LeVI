package com.lv.levi.auth; // Root package = Public

import java.util.UUID;

public record UserPrincipal(UUID id, String email) {}