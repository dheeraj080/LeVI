package com.lv.levi;

import com.lv.levi.auth.entity.User;
import com.lv.levi.category.entity.Category;
import com.lv.levi.category.repository.CategoryRepository;
import com.lv.levi.transaction.dto.TransactionDTO;
import com.lv.levi.transaction.entity.Transaction;
import com.lv.levi.transaction.entity.TransactionType;
import com.lv.levi.transaction.repository.TransactionRepository;
import com.lv.levi.transaction.service.TransactionService;
import com.tngtech.archunit.library.freeze.ViolationStore;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.Spy;
import org.mockito.junit.jupiter.MockitoExtension;
import org.mockito.verification.VerificationMode;
import org.modelmapper.ModelMapper;

import java.math.BigDecimal;
import java.util.Optional;
import java.util.UUID;

import static jdk.internal.classfile.impl.verifier.VerifierImpl.verify;
import static org.hamcrest.Matchers.any;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.when;

@ExtendWith(MockitoExtension.class)
class TransactionServiceTest {

    @Mock
    private TransactionRepository transactionRepository;

    @Mock
    private CategoryRepository categoryRepository;

    @Spy // Use Spy to use the real ModelMapper logic
    private ModelMapper modelMapper = new ModelMapper();

    @InjectMocks
    private TransactionService transactionService;

    @Test
    void getTotalExpense_ShouldReturnZero_WhenNoTransactions() {
        // Arrange
        User user = new User();
        when(transactionRepository.sumTotalByType(user, TransactionType.DEBIT))
                .thenReturn(null);

        // Act
        BigDecimal result = transactionService.getTotalExpense(user);

        // Assert
        assertEquals(BigDecimal.ZERO, result);
    }

    @Test
    void createTransaction_ShouldSaveAndReturnDto() {
        // Arrange
        User user = new User();
        UUID catId = UUID.randomUUID();
        Category category = Category.builder().id(catId).name("Food").build();

        TransactionDTO dto = TransactionDTO.builder()
                .title("Lunch")
                .amount(new BigDecimal("15.00"))
                .type(TransactionType.DEBIT)
                .categoryId(catId)
                .build();

        when(categoryRepository.findById(catId)).thenReturn(Optional.of(category));
        when(transactionRepository.save(any(Transaction.class))).thenAnswer(i -> i.getArguments()[0]);

        // Act
        TransactionDTO savedDto = transactionService.createTransaction(dto, user);

        // Assert
        assertNotNull(savedDto);
        assertEquals("Lunch", savedDto.getTitle());
        verify(transactionRepository, times(1)).save(any());
    }

    private ViolationStore verify(TransactionRepository transactionRepository, VerificationMode times) {
    }
}