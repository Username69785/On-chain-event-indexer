document.addEventListener('DOMContentLoaded', () => {
    const form = document.getElementById('indexerForm');
    const input = document.getElementById('addressInput');
    const statusMessage = document.getElementById('statusMessage');
    const submitBtn = document.getElementById('submitBtn');
    const btnText = submitBtn.querySelector('span');
    let isLoading = false;

    const handleAnalyze = async (e) => {
        e?.preventDefault();
        e?.stopPropagation();

        if (isLoading) {
            return;
        }

        const address = input.value.trim();

        if (!address) {
            showStatus('Please enter a valid address', 'error');
            return;
        }

        if (!isAddressLike(address)) {
            showStatus('Invalid input data: value is not an address', 'error');
            return;
        }

        const solanaValidation = validateSolanaAddress(address);
        if (!solanaValidation.isValid) {
            showStatus(solanaValidation.message, 'error');
            return;
        }

        // Simulate loading state
        setLoading(true);

        try {
            // Send address to backend
            const response = await fetch('http://127.0.0.1:8080/analyze', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({ address }),
            });

            if (!response.ok) {
                // Handle non-200 responses
                const errorData = await response.json().catch(() => ({}));
                throw new Error(errorData.message || 'Failed to start indexing');
            }

            const result = await response.json();
            console.log('Server response:', result);

            showStatus(`Successfully started indexing for: ${address.slice(0, 4)}...${address.slice(-4)}`, 'success');
            input.value = '';
        } catch (error) {
            console.error('Error:', error);
            showStatus('An error occurred. Please try again.', 'error');
        } finally {
            setLoading(false);
        }
    };

    form.addEventListener('submit', handleAnalyze);
    submitBtn.addEventListener('click', handleAnalyze);

    function isAddressLike(value) {
        // Basic pre-check: either EVM-like (0x...) or base58-like address.
        const evmLike = /^0x[a-fA-F0-9]{40}$/;
        const base58Like = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;

        return evmLike.test(value) || base58Like.test(value);
    }

    function validateSolanaAddress(value) {
        if (value.startsWith('0x') || value.startsWith('0X')) {
            return {
                isValid: false,
                message: 'Invalid address type: EVM (0x...) addresses are not supported',
            };
        }

        if (value.length < 32 || value.length > 44) {
            return {
                isValid: false,
                message: 'Invalid Solana address length (allowed: 32-44 chars)',
            };
        }

        // Solana base58 alphabet excludes 0, O, I, l; case is significant.
        if (/[0OIl]/.test(value)) {
            return {
                isValid: false,
                message: 'Invalid Solana address: contains forbidden characters (0, O, I, l)',
            };
        }

        if (!/^[1-9A-HJ-NP-Za-km-z]+$/.test(value)) {
            return {
                isValid: false,
                message: 'Invalid Solana address format',
            };
        }

        return { isValid: true, message: '' };
    }

    function showStatus(message, type) {
        statusMessage.textContent = message;
        statusMessage.className = `status-message ${type}`;
        statusMessage.classList.remove('hidden');

        // Auto-hide after 5 seconds
        if (type === 'success') {
            setTimeout(() => {
                statusMessage.classList.add('hidden');
            }, 5000);
        }
    }

    function setLoading(loading) {
        isLoading = loading;

        if (loading) {
            submitBtn.disabled = true;
            btnText.textContent = 'Processing...';
            submitBtn.classList.add('loading');
        } else {
            submitBtn.disabled = false;
            btnText.textContent = 'Analyze';
            submitBtn.classList.remove('loading');
        }
    }
});
