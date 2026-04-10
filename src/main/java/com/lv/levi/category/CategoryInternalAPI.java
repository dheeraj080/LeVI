package com.lv.levi.category;

import java.util.UUID;

public interface CategoryInternalAPI {
    boolean existsByIdAndUserId(UUID categoryId, UUID userId);
    CategoryMetadata getCategoryMetadata(UUID categoryId);
}