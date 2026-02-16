document.addEventListener('DOMContentLoaded', () => {
    const form = document.getElementById('indexerForm');
    const input = document.getElementById('addressInput');
    const statusMessage = document.getElementById('statusMessage');
    const submitBtn = document.getElementById('submitBtn');
    const btnText = submitBtn.querySelector('span');

    form.addEventListener('submit', async (e) => {
        e.preventDefault();

        const address = input.value.trim();

        if (!address) {
            showStatus('Please enter a valid address', 'error');
            return;
        }

        // Solana address validation (basic length check for MVP)
        if (address.length < 32 || address.length > 44) {
            showStatus('Invalid Solana address format', 'error');
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
    });

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

    function setLoading(isLoading) {
        if (isLoading) {
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
